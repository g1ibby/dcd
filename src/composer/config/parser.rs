use crate::composer::{
    errors::ComposerError,
    types::{ComposeFile, ComposerResult, EnvFiles, Service, Volume},
};
use serde_yaml::Value;
use std::collections::HashMap;

pub struct ConfigParser;

impl ConfigParser {
    /// Parse the output of `docker compose config` command
    pub fn parse_config(config_output: &str) -> ComposerResult<ComposeFile> {
        let yaml: Value = serde_yaml::from_str(config_output).map_err(ComposerError::YamlError)?;

        let mapping = yaml.as_mapping().ok_or_else(|| {
            ComposerError::parse_error("Docker Compose config must be a YAML mapping")
        })?;

        // Parse services (required)
        let services = mapping
            .get("services")
            .ok_or_else(|| ComposerError::parse_error("No services found in config"))?;
        let services_map = Self::parse_services(services)?;

        // Parse volumes (optional, but needed for volume mounts)
        let volumes = mapping
            .get("volumes")
            .map(Self::parse_volumes)
            .transpose()?;

        Ok(ComposeFile {
            services: services_map,
            volumes,
        })
    }

    fn parse_services(services: &Value) -> ComposerResult<HashMap<String, Service>> {
        let services_mapping = services
            .as_mapping()
            .ok_or_else(|| ComposerError::parse_error("Services must be a YAML mapping"))?;

        let mut result = HashMap::new();

        for (name, service_value) in services_mapping {
            let service_name = name
                .as_str()
                .ok_or_else(|| ComposerError::parse_error("Service name must be a string"))?;

            let service = serde_yaml::from_value(service_value.clone()).map_err(|e| {
                ComposerError::parse_error(format!(
                    "Failed to parse service '{}': {}",
                    service_name, e
                ))
            })?;

            result.insert(service_name.to_string(), service);
        }

        Ok(result)
    }

    fn parse_volumes(volumes: &Value) -> ComposerResult<HashMap<String, Volume>> {
        let volumes_mapping = volumes
            .as_mapping()
            .ok_or_else(|| ComposerError::parse_error("Volumes must be a YAML mapping"))?;

        let mut result = HashMap::new();

        for (name, volume_value) in volumes_mapping {
            let volume_name = name
                .as_str()
                .ok_or_else(|| ComposerError::parse_error("Volume name must be a string"))?;

            let volume = serde_yaml::from_value(volume_value.clone()).map_err(|e| {
                ComposerError::parse_error(format!(
                    "Failed to parse volume '{}': {}",
                    volume_name, e
                ))
            })?;

            result.insert(volume_name.to_string(), volume);
        }

        Ok(result)
    }

    /// Extract all local file references from the config
    pub fn extract_local_references(compose_file: &ComposeFile) -> Vec<String> {
        let mut references = Vec::new();

        for service in compose_file.services.values() {
            // Check build context
            if let Some(build) = &service.build {
                references.push(build.context.to_string_lossy().into_owned());
                if let Some(dockerfile) = &build.dockerfile {
                    references.push(dockerfile.to_string_lossy().into_owned());
                }
            }

            // Check for additional config files
            if let Some(configs) = &service.configs {
                for config in configs {
                    if let Some(source) = &config.source {
                        references.push(source.to_string_lossy().into_owned());
                    }
                }
            }

            // Check env_file directives - already resolved by docker compose config
            if let Some(env_files) = &service.env_file {
                match env_files {
                    EnvFiles::Single(file) => {
                        references.push(file.clone());
                    }
                    EnvFiles::Multiple(files) => {
                        references.extend(files.iter().cloned());
                    }
                }
            }

            // Check bind mount volumes for local paths
            if let Some(volumes) = &service.volumes {
                for volume in volumes {
                    if volume.r#type == "bind" {
                        if let Some(source) = &volume.source {
                            // Only add non-empty source paths
                            if !source.is_empty() {
                                references.push(source.clone());
                            }
                        }
                    }
                }
            }
        }

        references
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_basic_config() {
        let config = r#"
services:
  db:
    build:
      context: /Users/user/code/tools/dcd/db
      dockerfile: Dockerfile.db
    networks:
      default: null
    volumes:
      - type: volume
        source: data
        target: /var/lib/postgresql/data
        volume: {}
  web:
    environment:
      NODE_ENV: production
    image: nginx:latest
    networks:
      default: null
    ports:
      - mode: ingress
        target: 80
        published: "80"
        protocol: tcp
networks:
  default:
    name: dcd_default
volumes:
  data:
    name: data
    external: true
"#;

        let result = ConfigParser::parse_config(config).unwrap();

        // Check services
        assert_eq!(result.services.len(), 2);

        // Check web service
        let web = result.services.get("web").unwrap();
        assert_eq!(web.image, Some("nginx:latest".to_string()));

        // Check db service
        let db = result.services.get("db").unwrap();
        assert!(db.build.is_some());

        // Check volumes
        assert!(result.volumes.is_some());
        let volumes = result.volumes.unwrap();
        assert!(volumes.contains_key("data"));
    }

    #[test]
    fn test_parse_config_with_local_references() {
        let config = r#"
services:
  app:
    build:
      context: ./app
      dockerfile: Dockerfile.dev
    configs:
      - source: ./config/app.conf
        target: /etc/app/config.conf
  api:
    build:
      context: ../api
    configs:
      - source: ./config/api.yaml
        target: /etc/api/config.yaml
"#;

        let compose_file = ConfigParser::parse_config(config).unwrap();
        let references = ConfigParser::extract_local_references(&compose_file);

        assert!(references.contains(&"./app".to_string()));
        assert!(references.contains(&"Dockerfile.dev".to_string()));
        assert!(references.contains(&"../api".to_string()));
        assert!(references.contains(&"./config/app.conf".to_string()));
        assert!(references.contains(&"./config/api.yaml".to_string()));
    }

    #[test]
    fn test_parse_config_with_volumes() {
        let config = r#"
services:
  app:
    volumes:
      - type: bind
        source: ./data
        target: /app/data
      - type: volume
        source: mydata
        target: /data
volumes:
  mydata:
    name: external_volume
"#;

        let result = ConfigParser::parse_config(config).unwrap();

        // Check service volumes
        let app = result.services.get("app").unwrap();
        let volumes = app.volumes.as_ref().unwrap();
        assert_eq!(volumes.len(), 2);

        // Check named volumes
        let named_volumes = result.volumes.unwrap();
        assert_eq!(named_volumes.len(), 1);
        assert!(named_volumes.contains_key("mydata"));
    }

    #[test]
    fn test_parse_invalid_config() {
        let invalid_configs = vec![
            // Empty config
            "",
            // Missing services
            "volumes:\n  data:",
            // Invalid YAML
            "services: - invalid",
            // Invalid service definition
            r#"
services:
  web: invalid
            "#,
        ];

        for config in invalid_configs {
            assert!(ConfigParser::parse_config(config).is_err());
        }
    }

    #[test]
    fn test_extract_local_references_empty() {
        let config = r#"
services:
  web:
    image: nginx:latest
"#;

        let compose_file = ConfigParser::parse_config(config).unwrap();
        let references = ConfigParser::extract_local_references(&compose_file);

        assert!(references.is_empty());
    }

    #[test]
    fn test_parse_config_with_complex_volumes() {
        let config = r#"
services:
  app:
    volumes:
      - type: bind
        source: ./src
        target: /app/src
        read_only: true
      - type: volume
        source: node_modules
        target: /app/node_modules
      - type: tmpfs
        target: /app/temp

volumes:
  node_modules:
    driver: local
    driver_opts:
      type: none
      device: ./node_modules
      o: bind
"#;

        let result = ConfigParser::parse_config(config).unwrap();

        // Check service volumes
        let app = result.services.get("app").unwrap();
        let volumes = app.volumes.as_ref().unwrap();
        assert_eq!(volumes.len(), 3);

        // Check volume definitions
        let named_volumes = result.volumes.unwrap();
        assert_eq!(named_volumes.len(), 1);
        assert!(named_volumes.contains_key("node_modules"));
    }
}

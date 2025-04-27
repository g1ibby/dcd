use crate::composer::{
    errors::ComposerError,
    types::{ComposerResult, ComposerVariables},
};

pub struct VariablesParser;

impl VariablesParser {
    /// Parses the output of `docker compose config --variables`
    /// Example input:
    /// NAME         REQUIRED     DEFAULT VALUE  ALTERNATE VALUE
    /// PG_PASS      true
    /// DB_PORT      false        5432
    pub fn parse_variables_output(output: &str) -> ComposerResult<Vec<ComposerVariables>> {
        let mut variables = Vec::new();
        let lines: Vec<&str> = output.lines().collect();

        // Need at least header line
        if lines.is_empty() {
            return Ok(variables);
        }

        // Skip header line
        for line in lines.iter().skip(1) {
            let var = Self::parse_variable_line(line)?;
            if let Some(variable) = var {
                variables.push(variable);
            }
        }

        Ok(variables)
    }

    fn parse_variable_line(line: &str) -> ComposerResult<Option<ComposerVariables>> {
        let line = line.trim();
        if line.is_empty() {
            return Ok(None);
        }

        // Split by whitespace but preserve multiple spaces
        let parts: Vec<&str> = line.split_whitespace().collect();

        if parts.is_empty() {
            return Ok(None);
        }

        // Need at least name and required fields
        if parts.len() < 2 {
            return Err(ComposerError::parse_error(format!(
                "Invalid variable line format: {}",
                line
            )));
        }

        let variable = ComposerVariables {
            name: parts[0].to_string(),
            required: parts[1].parse::<bool>().map_err(|_| {
                ComposerError::parse_error(format!("Invalid required field value: {}", parts[1]))
            })?,
            default_value: parts.get(2).map(|s| s.to_string()),
            alternate_value: parts.get(3).map(|s| s.to_string()),
        };

        Ok(Some(variable))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_empty_output() {
        let result = VariablesParser::parse_variables_output("").unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_valid_variables() {
        let output = r#"NAME         REQUIRED     DEFAULT VALUE  ALTERNATE VALUE
PG_PASS      true
DB_PORT      false        5432
REDIS_HOST   false        localhost    redis"#;

        let variables = VariablesParser::parse_variables_output(output).unwrap();

        assert_eq!(variables.len(), 3);

        // Check PG_PASS
        assert_eq!(variables[0].name, "PG_PASS");
        assert!(variables[0].required);
        assert!(variables[0].default_value.is_none());
        assert!(variables[0].alternate_value.is_none());

        // Check DB_PORT
        assert_eq!(variables[1].name, "DB_PORT");
        assert!(!variables[1].required);
        assert_eq!(variables[1].default_value.as_deref(), Some("5432"));
        assert!(variables[1].alternate_value.is_none());

        // Check REDIS_HOST
        assert_eq!(variables[2].name, "REDIS_HOST");
        assert!(!variables[2].required);
        assert_eq!(variables[2].default_value.as_deref(), Some("localhost"));
        assert_eq!(variables[2].alternate_value.as_deref(), Some("redis"));
    }

    #[test]
    fn test_parse_invalid_line() {
        let output = "NAME         REQUIRED     DEFAULT VALUE\nINVALID_VAR";
        let result = VariablesParser::parse_variables_output(output);
        assert!(result.is_err());
    }
}

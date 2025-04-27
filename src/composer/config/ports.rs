use crate::composer::types::{ComposerResult, PortMapping};

pub struct PortsParser;

impl PortsParser {
    pub fn parse_ports(ports: &[PortMapping]) -> ComposerResult<Vec<PortMapping>> {
        Ok(ports.to_vec())
    }
}

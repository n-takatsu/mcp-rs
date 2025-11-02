use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub enum Transport {
    /// Standard input/output transport for MCP clients
    #[default]
    Stdio,
    /// TCP transport for development and testing
    Tcp { address: String, port: u16 },
}

impl Transport {
    pub fn from_args(stdio: bool, address: String, port: u16) -> Self {
        if stdio {
            Transport::Stdio
        } else {
            Transport::Tcp { address, port }
        }
    }

    pub fn is_stdio(&self) -> bool {
        matches!(self, Transport::Stdio)
    }

    pub fn is_tcp(&self) -> bool {
        matches!(self, Transport::Tcp { .. })
    }

    pub fn address(&self) -> Option<String> {
        match self {
            Transport::Tcp { address, port } => Some(format!("{}:{}", address, port)),
            Transport::Stdio => None,
        }
    }
}

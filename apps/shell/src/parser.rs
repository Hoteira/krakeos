use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;

#[derive(PartialEq)]
pub enum RedirType {
    None,
    Output(String),
    Append(String),
    Input(String),
}

pub struct CmdSeg {
    pub cmd: String,
    pub args: Vec<String>,
    pub input_file: Option<String>,
    pub output_file: Option<String>,
    pub append_mode: bool,
}

pub fn parse_segment(segment: &str) -> CmdSeg {
    let mut parts = segment.split_whitespace();
    let mut cmd = String::new();
    let mut args = Vec::new();
    let mut input_file = None;
    let mut output_file = None;
    let mut append_mode = false;

    let mut pending_redirect = RedirType::None;

    for part in parts {
        match pending_redirect {
            RedirType::Output(_) => {
                output_file = Some(part.to_string());
                pending_redirect = RedirType::None;
            }
            RedirType::Append(_) => {
                output_file = Some(part.to_string());
                append_mode = true;
                pending_redirect = RedirType::None;
            }
            RedirType::Input(_) => {
                input_file = Some(part.to_string());
                pending_redirect = RedirType::None;
            }
            RedirType::None => {
                if part == ">" {
                    pending_redirect = RedirType::Output(String::new());
                } else if part == ">>" {
                    pending_redirect = RedirType::Append(String::new());
                } else if part == "<" {
                    pending_redirect = RedirType::Input(String::new());
                } else if cmd.is_empty() {
                    cmd = part.to_string();
                } else {
                    args.push(part.to_string());
                }
            }
        }
    }

    CmdSeg { cmd, args, input_file, output_file, append_mode }
}

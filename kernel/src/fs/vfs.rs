use alloc::boxed::Box;
use alloc::vec::Vec;
use alloc::string::String;
use alloc::string::ToString;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Path {
    pub disk_id: u8,
    pub components: Vec<String>,
}

impl Path {
    pub fn parse(path_str: &str) -> Result<Self, String> {
        if !path_str.starts_with('@') {
            return Err(String::from("Path must start with '@' (e.g., @0/path/to/file)"));
        }

        // Find the end of the disk identifier (first '/')
        let disk_end = path_str.find('/').ok_or(String::from("Invalid path format: missing '/' after disk ID"))?;
        
        let disk_part = &path_str[1..disk_end];
        let path_part = &path_str[disk_end+1..];

        // Parse disk ID (support 0x prefix for hex, or default decimal)
        let disk_id = if disk_part.starts_with("0x") || disk_part.starts_with("0X") {
            u8::from_str_radix(&disk_part[2..], 16).map_err(|_| String::from("Invalid hex disk ID"))?
        } else {
            disk_part.parse::<u8>().map_err(|_| String::from("Invalid decimal disk ID"))?
        };

        // Split path into components, filtering empty strings (e.g., from "///")
        let components: Vec<String> = path_part
            .split('/')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();

        Ok(Path {
            disk_id,
            components,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileType {
    File,
    Directory,
    Device,
    Unknown,
}

pub trait FileSystem {
    fn root(&mut self) -> Result<Box<dyn VfsNode>, String>;
}

pub trait VfsNode {
    fn name(&self) -> String;
    fn size(&self) -> u64;
    fn kind(&self) -> FileType;
    
    // File operations
    fn read(&mut self, offset: u64, buffer: &mut [u8]) -> Result<usize, String>;
    fn write(&mut self, offset: u64, buffer: &[u8]) -> Result<usize, String>;
    
    // Directory operations
    fn children(&mut self) -> Result<Vec<Box<dyn VfsNode>>, String>;
    fn find(&mut self, name: &str) -> Result<Box<dyn VfsNode>, String>;
}

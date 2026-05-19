// src/io/sqr.rs
use std::io::{Read, Write};
use std::path::Path;
use crate::project::Project;

const MAGIC: &[u8; 4] = b"SQR\0";
const VERSION: u8 = 1;

pub fn save_sqr(project: &Project, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let encoded = bincode::serialize(project)?;
    let compressed = lz4_flex::compress_prepend_size(&encoded);
    let mut file = std::fs::File::create(path)?;
    file.write_all(MAGIC)?;
    file.write_all(&[VERSION])?;
    file.write_all(&compressed)?;
    Ok(())
}

pub fn load_sqr(path: &Path) -> Result<Project, Box<dyn std::error::Error>> {
    let mut file = std::fs::File::open(path)?;
    let mut magic = [0u8; 4];
    file.read_exact(&mut magic)?;
    if &magic != MAGIC {
        return Err("Invalid .sqr file: bad magic bytes".into());
    }
    let mut version = [0u8; 1];
    file.read_exact(&mut version)?;
    if version[0] != VERSION {
        return Err(format!("Unsupported .sqr version: {}", version[0]).into());
    }
    let mut compressed = Vec::new();
    file.read_to_end(&mut compressed)?;
    let decoded = lz4_flex::decompress_size_prepended(&compressed)?;
    let project: Project = bincode::deserialize(&decoded)?;
    Ok(project)
}

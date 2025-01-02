use crate::commands::Location;
use gimli::{
    AttributeValue, DebuggingInformationEntry, Dwarf, DwarfFileType, EndianSlice, RunTimeEndian,
    Unit, UnitHeader, UnitOffset,
};
use object::{
    read::{ObjectSection, ReadCache, ReadRef},
    Object,
};
use rustc_demangle::demangle;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::mem;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, RwLock};
use thiserror::Error;
use tracing::{debug, error, trace, warn};

/// So I think if I:
///
/// 1. Mutate the vectors
/// 2. Remove hashmap values
///
/// Then this could explode. But if I don't do those things... Maybe I'm fine?
static LOADED_FILES: LazyLock<RwLock<HashMap<PathBuf, Arc<Vec<u8>>>>> =
    LazyLock::new(|| Default::default());

#[derive(Debug, Error)]
pub enum ObjectError {
    #[error("can't open ELF file")]
    CantOpenElf,
    #[error("couldn't parse ELF file")]
    CouldntParse,
    #[error("couldn't find location in ELF file")]
    BadLocation,
    #[error("error when parsing DWARF tables")]
    DwarfParsingFailed,
    #[error("missing {0}")]
    SectionMissing(&'static str),
    #[error("couldn't read data from {0}")]
    CouldntReadSectionData(&'static str),
    #[error("failed to parse debug information tree")]
    FailedToParseDieTree,
}

#[derive(Debug)]
pub struct ExecutableFile {
    elf_file: object::File<'static, &'static [u8]>,
    dwarf: Dwarf<EndianSlice<'static, RunTimeEndian>>,
}

fn cache_file(path: &Path) -> io::Result<()> {
    let rw_lock = &*LOADED_FILES;

    let mut cache = rw_lock.write().unwrap();
    if !cache.contains_key(path) {
        let data = fs::read(path)?;
        cache.insert(path.to_path_buf(), Arc::new(data));
    }
    Ok(())
}

fn get_bytes(path: &Path) -> Option<Arc<Vec<u8>>> {
    (&*LOADED_FILES).read().unwrap().get(path).map(Arc::clone)
}

fn try_get_file_section_reader(
    section_id: gimli::SectionId,
    endian: RunTimeEndian,
    object: &object::File<'static, &'static [u8]>,
) -> Result<EndianSlice<'static, RunTimeEndian>, ObjectError> {
    let data = object
        .section_by_name(section_id.name())
        .ok_or(ObjectError::SectionMissing(section_id.name()))?;
    let data = data.data().map_err(|e| {
        error!("Couldn't access section data {}", e);
        ObjectError::CouldntReadSectionData(section_id.name())
    })?;
    Ok(EndianSlice::new(data, endian))
}

fn get_file_section_reader(
    section_id: gimli::SectionId,
    endian: RunTimeEndian,
    object: &object::File<'static, &'static [u8]>,
) -> Result<EndianSlice<'static, RunTimeEndian>, ObjectError> {
    if let Ok(section) = try_get_file_section_reader(section_id, endian, object) {
        Ok(section)
    } else {
        warn!(
            "Couldn't get {}, replacing with empty buffer",
            section_id.name()
        );
        Ok(EndianSlice::new(&[], endian))
    }
}

impl ExecutableFile {
    pub fn load(path: &Path) -> Result<Self, ObjectError> {
        let file = cache_file(path).map_err(|e| {
            error!("Couldn't open {}: {}", path.display(), e);
            ObjectError::CantOpenElf
        })?;

        let data = get_bytes(path).unwrap();
        let elf_file = object::File::parse(unsafe { mem::transmute(data.as_ref().as_slice()) })
            .map_err(|e| {
                error!("Couldn't parse elf file: {}", e);
                ObjectError::CouldntParse
            })?;

        let endian = if elf_file.is_little_endian() {
            RunTimeEndian::Little
        } else {
            RunTimeEndian::Big
        };

        let loader =
            |section: gimli::SectionId| get_file_section_reader(section, endian, &elf_file);
        let mut dwarf = gimli::Dwarf::load(loader)?;
        dwarf.file_type = DwarfFileType::Main;

        Ok(ExecutableFile { elf_file, dwarf })
    }

    pub fn get_address(&self, location: Location) -> Result<u64, ObjectError> {
        match location {
            Location::Address(addr) => Ok(addr),
            Location::Line { file, line } => todo!(),
            Location::Function(fn_name) => todo!(),
        }
    }

    pub fn endianness(&self) -> RunTimeEndian {
        if self.elf_file.is_little_endian() {
            RunTimeEndian::Little
        } else {
            RunTimeEndian::Big
        }
    }

    fn compile_unit_containing_address(
        &self,
        address: u64,
    ) -> Option<Unit<EndianSlice<'static, RunTimeEndian>>> {
        let mut units = self.dwarf.units();
        while let Ok(Some(header)) = units.next() {
            if let Ok(unit) = self.dwarf.unit(header) {
                let mut ranges = match self.dwarf.unit_ranges(&unit) {
                    Ok(ranges) => ranges,
                    Err(e) => {
                        error!("Couldn't get debug ranges for unit: {}", e);
                        continue;
                    }
                };
                while let Ok(Some(r)) = ranges.next() {
                    if (r.begin..r.end).contains(&address) {
                        return Some(unit);
                    };
                }
            }
        }
        None
    }

    fn function_containing_address<'a>(
        &self,
        address: u64,
    ) -> Result<Option<(Unit<EndianSlice<'static, RunTimeEndian>>, UnitOffset)>, ObjectError> {
        let cu = match self.compile_unit_containing_address(address) {
            Some(cu) => cu,
            None => return Ok(None),
        };

        let mut cursor = cu.entries();

        while let Some((delta_depth, current)) = cursor
            .next_dfs()
            .map_err(|_| ObjectError::FailedToParseDieTree)?
        {
            if current.tag() == gimli::DW_TAG_subprogram {
                // I am a function!
                let low_pc = current.attr_value(gimli::DW_AT_low_pc);
                let high_pc = current.attr_value(gimli::DW_AT_high_pc);
                let low_pc = match low_pc {
                    Ok(Some(AttributeValue::Addr(x))) => x,
                    _ => 0u64,
                };
                // High is an offset from the base pc, therefore is u64 data.
                let high_pc = match high_pc {
                    Ok(Some(AttributeValue::Udata(x))) => x,
                    _ => 0u64,
                };
                if (low_pc..high_pc).contains(&address) {
                    let offset = current.offset();
                    return Ok(Some((cu, offset)));
                }
            }
        }
        Ok(None)
    }

    fn find_functions(
        &self,
        name: &str,
    ) -> Result<
        Vec<DebuggingInformationEntry<'static, 'static, EndianSlice<'static, RunTimeEndian>>>,
        ObjectError,
    > {
        todo!()
    }
}

// TODO could we be cheeky and load our test binary in the tests and look for the test functions
// themselves!
#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn doesnt_just_segfault() {
        let path = env::current_exe().unwrap();

        let file = ExecutableFile::load(&path).unwrap();

        file.endianness();
        assert!(file.elf_file.symbols().count() > 0);
    }
}

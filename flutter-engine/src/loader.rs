use crate::FlutterAotSnapshot;
use libloading::Library;
use std::io::Error;
use std::path::Path;
use xmas_elf::sections::SectionData;
use xmas_elf::symbol_table::Entry;
use xmas_elf::ElfFile;

pub unsafe fn load_snapshot(aot_path: &Path) -> Result<(Library, FlutterAotSnapshot), Error> {
    let mut vm_snapshot_data_size = 0;
    let mut vm_snapshot_instructions_size = 0;
    let mut isolate_snapshot_data_size = 0;
    let mut isolate_snapshot_instructions_size = 0;

    let bytes = std::fs::read(aot_path)?;
    let elf = ElfFile::new(&bytes).unwrap();
    let dynsym = elf
        .find_section_by_name(".dynsym")
        .expect("app.so contains .dynsym section");
    if let SectionData::DynSymbolTable64(entries) = dynsym.get_data(&elf).unwrap() {
        for entry in entries {
            match entry.get_name(&elf).unwrap() {
                "_kDartVmSnapshotData" => {
                    vm_snapshot_data_size = entry.size() as _;
                }
                "_kDartVmSnapshotInstructions" => {
                    vm_snapshot_instructions_size = entry.size() as _;
                }
                "_kDartIsolateSnapshotData" => {
                    isolate_snapshot_data_size = entry.size() as _;
                }
                "_kDartIsolateSnapshotInstructions" => {
                    isolate_snapshot_instructions_size = entry.size() as _;
                }
                _ => {}
            }
        }
    };
    let lib = Library::new(aot_path)?;
    let vm_snapshot_data = lib
        .get::<*const u8>(b"_kDartVmSnapshotData")?
        .into_raw()
        .into_raw() as _;
    let vm_snapshot_instructions = lib
        .get::<*const u8>(b"_kDartVmSnapshotInstructions")?
        .into_raw()
        .into_raw() as _;
    let isolate_snapshot_data = lib
        .get::<*const u8>(b"_kDartIsolateSnapshotData")?
        .into_raw()
        .into_raw() as _;
    let isolate_snapshot_instructions = lib
        .get::<*const u8>(b"_kDartIsolateSnapshotInstructions")?
        .into_raw()
        .into_raw() as _;
    let snapshot = FlutterAotSnapshot {
        vm_snapshot_data,
        vm_snapshot_data_size,
        vm_snapshot_instructions,
        vm_snapshot_instructions_size,
        isolate_snapshot_data,
        isolate_snapshot_data_size,
        isolate_snapshot_instructions,
        isolate_snapshot_instructions_size,
    };
    Ok((lib, snapshot))
}

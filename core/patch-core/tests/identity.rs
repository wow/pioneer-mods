use patch_core::identify_firmware;
use std::fs::File;
use std::io::Write;

#[test]
fn identifies_file_size_and_sha256() {
    let tempdir = tempfile::tempdir().expect("create tempdir");
    let input_path = tempdir.path().join("XDJ700.UPD");
    let mut file = File::create(&input_path).expect("create temp file");
    file.write_all(b"abc").expect("write temp file");
    file.flush().expect("flush temp file");

    let identity = identify_firmware(&input_path).expect("identify firmware");

    assert_eq!(identity.file_name, "XDJ700.UPD");
    assert_eq!(identity.size_bytes, 3);
    assert_eq!(
        identity.sha256_hex,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

// Own fixture built with the .NET Framework csc.exe:
// csc /nologo /target:library /out:fixtures/host-version.bin fixtures/host-version.cs
// Contains only HostVersionFixture and the Windows file-version resource.
pub fn host_fixture(major: u32, minor: u32, patch: u32) -> Vec<u8> {
    let mut bytes = include_bytes!("host-version.bin").to_vec();
    let fixed = bytes
        .windows(8)
        .position(|b| b == [0xbd, 0x04, 0xef, 0xfe, 0, 0, 1, 0])
        .unwrap();
    for (offset, value) in [
        (8, (major << 16) | minor),
        (12, patch << 16),
        (16, (major << 16) | minor),
        (20, patch << 16),
    ] {
        bytes[fixed + offset..fixed + offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    bytes
}

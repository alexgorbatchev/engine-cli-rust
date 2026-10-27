pub fn scratch() -> tempfile::TempDir {
    std::fs::create_dir_all(".tmp").unwrap();
    tempfile::tempdir_in(".tmp").unwrap()
}

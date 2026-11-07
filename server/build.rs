fn main() {
    // Recompila quando uma migração mudar (as migrações são embutidas no binário).
    println!("cargo:rerun-if-changed=migrations");
}

pub mod apply;
pub mod dns;
pub mod ingress;
pub mod interactive;
pub mod rollback;
pub mod tunnel;

#[derive(Debug, Clone)]
pub struct MutateOptions {
    pub yes: bool,
    pub dry_run: bool,
    pub allow_create: bool,
    pub allow_delete: bool,
    pub allow_insecure_origin: bool,
    pub expect_version: Option<i64>,
    pub expect_sha256: Option<String>,
}

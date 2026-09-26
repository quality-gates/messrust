//! Reproducer for Bug 2: UnusedFormalParameter falsely flags abstract trait method parameters.

pub struct User;
pub struct Context;
pub struct Error;

pub trait Repository {
    // Bug: Abstract trait method declarations (no body) flag parameter names as unused.
    fn find_by_id(&self, id: u64) -> Option<User>;
    fn save(&self, user: &User, ctx: &Context) -> Result<(), Error>;

    // Control: Trait method WITH default body that uses parameter is NOT flagged.
    fn default_helper(&self, prefix: &str) -> String {
        format!("{prefix}: default")
    }
}

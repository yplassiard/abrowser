pub mod log;

#[macro_export]
macro_rules! try_block {
    ($expr:expr) => {
        (|| Some($expr))()
    };
}

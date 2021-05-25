# threaded_logger

A logger wrapper that spawns `tokio` threads to make logging asynchronous.

## Usage

It must be used with another logger crate that implements the `log::Log` trait. This crate only provides a wrapper function.

Also, a `tokio` runtime must be used.

## Example

For example, you can use it with the `env_logger` crate.

`Cargo.toml`:

```toml
[dependencies]
log = "0.4.0"
env_logger = "0.8.3"
threaded_logger = "0.1.0"
tokio = { version = "1.0", features = ["full"] }
```

`main.rs`:

```rust
#[tokio::main]
async fn main() {
    let logger = env_logger::builder().build();
    let filter = logger.filter();

    threaded_logger::init(logger, filter);

    log::info!("hello");
}
```

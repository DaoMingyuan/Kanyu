// macOS 手工构建扩展模块需 `-undefined dynamic_lookup` 链接参数
// （pyo3 指南「Building and distribution → macOS」；maturin/setuptools-rust
// 会自动注入，裸 cargo build 不会）。其余平台此调用为空操作。
// 注意：不能走 .cargo/config.toml 的 rustflags——CI 环境变量
// RUSTFLAGS=-D warnings 会整体覆盖配置项 rustflags，链接参数只能经
// build.rs 下发（cargo:rustc-link-arg 作用于本 crate 最终产物）。
fn main() {
    pyo3_build_config::add_extension_module_link_args();
}

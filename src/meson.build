cargo_bin = find_program(
  'cargo',
  dirs: ['/usr/lib/sdk/rust-stable/bin'],
  required: true,
)

custom_target(
  'cargo-build',
  build_by_default: true,
  build_always_stale: true,
  output: meson.project_name(),
  console: true,
  install: true,
  install_dir: get_option('bindir'),
  command: [
    meson.project_source_root() / 'build-aux' / 'cargo.sh',
    meson.project_build_root(),
    meson.project_source_root(),
    '@OUTPUT@',
    get_option('buildtype'),
  ],
)

test(
  'cargo-test',
  cargo_bin,
  args: ['test', '--manifest-path', meson.project_source_root() / 'Cargo.toml'],
  timeout: 120,
)

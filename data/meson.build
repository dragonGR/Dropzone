desktop_file = configure_file(
  input: 'io.github.dragonGR.Dropzone.desktop.in',
  output: base_id + '.desktop',
  configuration: conf,
  install: true,
  install_dir: get_option('datadir') / 'applications',
)

desktop_utils = find_program('desktop-file-validate', required: false)
if desktop_utils.found()
  test('Validate desktop file', desktop_utils, args: [desktop_file])
endif

appstream_file = configure_file(
  input: 'io.github.dragonGR.Dropzone.metainfo.xml.in',
  output: base_id + '.metainfo.xml',
  configuration: conf,
  install: true,
  install_dir: get_option('datadir') / 'metainfo',
)

appstreamcli = find_program('appstreamcli', required: false)
if appstreamcli.found()
  test('Validate appstream file', appstreamcli, args: ['validate', '--no-net', appstream_file])
endif

install_data(
  'icons' / 'hicolor' / 'scalable' / 'apps' / (base_id + '.svg'),
  install_dir: get_option('datadir') / 'icons' / 'hicolor' / 'scalable' / 'apps',
)

install_data(
  'icons' / 'hicolor' / 'symbolic' / 'apps' / (base_id + '-symbolic.svg'),
  install_dir: get_option('datadir') / 'icons' / 'hicolor' / 'symbolic' / 'apps',
)

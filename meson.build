project(
  'dropzone',
  version: '1.1.0',
  meson_version: '>= 0.64.0',
  license: 'GPL-3.0-or-later',
)

gnome = import('gnome')
i18n = import('i18n')

base_id = 'io.github.dragonGR.Dropzone'

conf = configuration_data()
conf.set('APP_ID', base_id)
conf.set('VERSION', meson.project_version())
conf.set('GETTEXT_PACKAGE', meson.project_name())
conf.set('LOCALEDIR', get_option('prefix') / get_option('localedir'))
conf.set('PKGDATADIR', get_option('prefix') / get_option('datadir') / meson.project_name())

dependency('gtk4', version: '>= 4.16')
dependency('libadwaita-1', version: '>= 1.6')

subdir('po')
subdir('data')
subdir('src')

gnome.post_install(
  gtk_update_icon_cache: true,
  update_desktop_database: true,
)

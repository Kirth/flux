{
  lib,
  rustPlatform,
  pkg-config,
  glib,
  gettext,
  wrapGAppsHook4,
  gtk4,
  libadwaita,
  vte-gtk4,
  poppler,
  cairo,
  pango,
  gdk-pixbuf,
  graphene,
  fontconfig,
  freetype,
  xz,
  bzip2,
  zstd,
  openssl,
  glib-networking,
  gsettings-desktop-schemas,
  gst_all_1,
  # Tools flux runs at runtime (archives, thumbnails, devices, open-with).
  p7zip,
  binutils,
  file,
  exiftool,
  ffmpeg,
  icoutils,
  util-linux,
  udisks,
  xdg-utils,
  e2fsprogs,
  exfatprogs,
  dosfstools,
  btrfs-progs,
  xfsprogs,
  python3,
}:

let
  cargoToml = lib.importTOML ../Cargo.toml;
in
rustPlatform.buildRustPackage {
  pname = "flux";
  inherit (cargoToml.package) version;

  src = lib.cleanSource ./..;
  cargoLock.lockFile = ../Cargo.lock;

  nativeBuildInputs = [
    pkg-config
    glib # glib-compile-resources (build.rs)
    gettext # msgfmt (translations)
    wrapGAppsHook4
  ];

  buildInputs = [
    gtk4
    libadwaita
    vte-gtk4
    poppler
    cairo
    pango
    gdk-pixbuf
    graphene
    fontconfig
    freetype
    xz
    bzip2
    zstd
    openssl
    glib-networking
    gsettings-desktop-schemas
  ]
  ++ (with gst_all_1; [
    gstreamer
    gst-plugins-base
    gst-plugins-good
    gst-plugins-bad
    gst-plugins-ugly
    gst-libav
  ]);

  # Several tests depend on the host (mounts, $HOME, real user); run them with
  # `cargo test` in the dev shell instead.
  doCheck = false;

  # Mirrors `make install`, minus the network theme sync and the per-user steps.
  postInstall = ''
    share=$out/share
    install -Dm644 flux.svg $share/icons/hicolor/scalable/apps/io.github.killown.flux.svg
    mkdir -p $share/applications
    sed "s|@BIN_PATH@|$out/bin/flux-fm|g" flux.desktop.in \
      > $share/applications/io.github.killown.flux.desktop
    install -Dm644 packaging/flatpak/io.github.killown.flux.metainfo.xml \
      $share/metainfo/io.github.killown.flux.metainfo.xml

    mkdir -p $share/flux/themes $share/flux/menus $share/flux/icons $share/flux/scripts
    cp -r themes/. $share/flux/themes/
    if [ -d menus ]; then cp -r menus/. $share/flux/menus/; fi
    if [ -f template.svg ]; then install -m644 template.svg $share/flux/icons/template.svg; fi
    if [ -f assets/nerd_fonts.json ]; then install -m644 assets/nerd_fonts.json $share/flux/nerd_fonts.json; fi
    install -m755 scripts/*.py $share/flux/scripts/

    for po in po/*.po; do
      lang=$(basename "$po" .po)
      mkdir -p $share/locale/$lang/LC_MESSAGES
      msgfmt -o $share/locale/$lang/LC_MESSAGES/flux.mo "$po"
    done
  '';

  preFixup = ''
    gappsWrapperArgs+=(--prefix PATH : ${
      lib.makeBinPath [
        p7zip
        binutils
        file
        exiftool
        ffmpeg
        icoutils
        util-linux
        udisks
        xdg-utils
        e2fsprogs
        exfatprogs
        dosfstools
        btrfs-progs
        xfsprogs
        python3
      ]
    })
  '';

  meta = {
    description = "Fast GTK4/libadwaita file manager";
    homepage = "https://github.com/killown/flux";
    license = lib.licenses.gpl3Only;
    mainProgram = "flux-fm";
    platforms = lib.platforms.linux;
  };
}

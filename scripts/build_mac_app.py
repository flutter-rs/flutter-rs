#!/usr/bin/env python3
import os
import toml
from shutil import copyfile, copytree, rmtree
import subprocess

toml_file = os.path.join(os.getcwd(), 'Cargo.toml')
meta = toml.loads(open(toml_file).read())
name = meta['package']['name']

DEBUG = False
PROJ_DIR = os.getcwd()
TARGET_DIR = os.path.join(PROJ_DIR, 'target')
APP_NAME = name + '.app'
APP_DIR = os.path.join(TARGET_DIR, APP_NAME)
IDENTIFIER = 'one.juju.flutter-rs'
FLUTTER_LIB_VER = meta['package']['metadata']['flutter']['version']
FLUTTER_ASSETS = os.path.join(PROJ_DIR, 'examples', 'gallery', 'flutter_assets')

# clear last output
rmtree(APP_DIR)

bin_dir = os.path.join(APP_DIR, 'Contents', 'MacOS')
frm_dir = os.path.join(APP_DIR, 'Contents', 'Frameworks')
res_dir = os.path.join(APP_DIR, 'Contents', 'Resources')

os.makedirs(bin_dir, exist_ok = True)
os.makedirs(res_dir, exist_ok = True)
copyfile(os.path.join(TARGET_DIR, 'debug' if DEBUG else 'release' , name), os.path.join(bin_dir, name))
subprocess.run(['chmod', '+x', os.path.join(bin_dir, name)], check = True)
copytree(os.path.join(PROJ_DIR, 'libs', FLUTTER_LIB_VER, 'FlutterEmbedder.framework'), os.path.join(frm_dir, 'FlutterEmbedder.framework'), symlinks = True)

# copy resources
copyfile(os.path.join(PROJ_DIR, 'assets', 'icon.icns'), os.path.join(res_dir, 'icon.icns'))
copyfile(os.path.join(PROJ_DIR, 'assets', 'icudtl.dat'), os.path.join(res_dir, 'icudtl.dat'))
copytree(FLUTTER_ASSETS, os.path.join(res_dir, 'flutter_assets'))

plist = '''<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleExecutable</key>
	<string>{name}</string>
	<key>CFBundleIconFile</key>
	<string>icon.icns</string>
	<key>CFBundleIdentifier</key>
	<string>{identifier}</string>
	<key>NSHighResolutionCapable</key>
	<true/>
	<key>LSUIElement</key>
	<true/>
</dict>
</plist>
'''.format(identifier = IDENTIFIER, name = name)
plist_file = open(os.path.join(TARGET_DIR, APP_NAME, 'Contents', 'Info.plist'), 'w+')
plist_file.write(plist)

# os.path.join(TARGET, )

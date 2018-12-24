import os
from shutil import copyfile, copytree, rmtree
import subprocess

def build(envs):
    APP_NAME = envs['NAME'] + '.app'
    APP_DIR = os.path.join(envs['TARGET_DIR'], 'debug' if envs['DEBUG'] else 'release', APP_NAME)

    # clear last output
    rmtree(APP_DIR, ignore_errors = True)

    bin_dir = os.path.join(APP_DIR, 'Contents', 'MacOS')
    frm_dir = os.path.join(APP_DIR, 'Contents', 'Frameworks')
    res_dir = os.path.join(APP_DIR, 'Contents', 'Resources')

    os.makedirs(bin_dir, exist_ok = True)
    os.makedirs(res_dir, exist_ok = True)
    copyfile(os.path.join(envs['TARGET_DIR'], 'debug' if envs['DEBUG'] else 'release' , envs['NAME']), os.path.join(bin_dir, envs['NAME']))
    subprocess.run(['chmod', '+x', os.path.join(bin_dir, envs['NAME'])], check = True)
    copytree(os.path.join(envs['RUST_PROJ_DIR'], 'libs', envs['FLUTTER_LIB_VER'], 'FlutterEmbedder.framework'), os.path.join(frm_dir, 'FlutterEmbedder.framework'), symlinks = True)

    # copy resources
    copyfile(os.path.join(envs['RUST_PROJ_DIR'], 'assets', 'icon.icns'), os.path.join(res_dir, 'icon.icns'))
    copyfile(os.path.join(envs['RUST_PROJ_DIR'], 'assets', 'icudtl.dat'), os.path.join(res_dir, 'icudtl.dat'))
    copytree(envs['FLUTTER_ASSETS'], os.path.join(res_dir, 'flutter_assets'))

    plist = plist_tmpl.format(identifier = envs['IDENTIFIER'], name = envs['NAME'])
    plist_file = open(os.path.join(APP_DIR, 'Contents', 'Info.plist'), 'w+')
    plist_file.write(plist)


plist_tmpl = '''<?xml version="1.0" encoding="UTF-8"?>
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
'''

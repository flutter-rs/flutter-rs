import os
import subprocess
import tempfile
from shutil import copyfile, copytree, rmtree

def prepare(envs):
    name = envs['NAME']
    output_dir = envs['OUTPUT_DIR']
    assets_dir = envs['RUST_ASSETS_DIR']
    return dict(
        NAME = name,
        VERSION = envs['VERSION'],
        FILE1 = os.path.join(output_dir, name + '.exe'),
        FILE2 = os.path.join(output_dir, 'flutter_engine.dll'),
        FILE3 = os.path.join(assets_dir, 'icudtl.dat'),
        ICON = os.path.join(assets_dir, 'icon.ico'),
        FLUTTER_ASSETS = envs['FLUTTER_ASSETS'],
        OUTPUT_FILE = os.path.join(output_dir, 'Installer.exe'),
    )

def build(envs):
    subprocess.run([
        'makensis',
        os.path.join(os.path.dirname(__file__), 'installer.nsi')
    ], env = envs)

    return envs['OUTPUT_FILE']


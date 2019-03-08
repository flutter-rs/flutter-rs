#!/usr/bin/env python3
import os, sys
import toml
import argparse
from lib import look_for_proj_dir, get_workspace_dir
import subprocess

FLUTTER = 'flutter.bat' if sys.platform == 'win32' else 'flutter'

def collect_env(args):
    PROJ_DIR = look_for_proj_dir(os.path.abspath(__file__), 'pubspec.yaml')
    RUST_PROJ_DIR = os.path.join(PROJ_DIR, 'rust')
    RUST_ASSETS_DIR = os.path.join(RUST_PROJ_DIR, 'assets')
    TOML_FILE = os.path.join(RUST_PROJ_DIR, 'Cargo.toml')
    META = toml.loads(open(TOML_FILE).read())
    NAME = META['package']['name']
    VERSION = META['package']['version']
    DESCRIPTION = META['package']['description']

    DEBUG = not args.release
    RELEASE = args.release

    WORKSPACE = get_workspace_dir(RUST_PROJ_DIR)
    if WORKSPACE is not None:
        # cargo put outputs in workspace target directory
        TARGET_DIR = os.path.join(WORKSPACE, 'target')
    else:
        TARGET_DIR = os.path.join(RUST_PROJ_DIR, 'target')
    OUTPUT_DIR = os.path.join(TARGET_DIR, 'debug' if DEBUG else 'release')
    FLUTTER_CONFIG = META['package']['metadata']['flutter']
    IDENTIFIER =  FLUTTER_CONFIG['identifier'] if 'identifier' in FLUTTER_CONFIG else 'one.juju.flutter-app'
    FLUTTER_LIB_VER = META['package']['metadata']['flutter']['version']
    FLUTTER_ASSETS = os.path.join(os.path.dirname(RUST_PROJ_DIR), 'build', 'flutter_assets')
    return locals()

def cargo_build(cwd, release = False):
    args = ['cargo', 'build']
    if release:
        args.append('--release')
    subprocess.run(args, cwd = cwd)

def build_flutter(envs):
    subprocess.run([FLUTTER, 'build', 'bundle'], cwd = envs['PROJ_DIR'])

if __name__ == '__main__':
    parser = argparse.ArgumentParser(prog='build', description='rust app distribution builder')
    parser.add_argument('dist', choices=['mac', 'dmg', 'nsis', 'snap'], help='distribution type')
    parser.add_argument('--release', action='store_true', help='build release package')

    args = parser.parse_args()
    envs = collect_env(args)

    print('>>> Building rust project')
    cargo_build(envs['RUST_PROJ_DIR'], envs['RELEASE'])

    print('>>> Building flutter bundle')
    build_flutter(envs)

    print('>>> Building package')
    # prepare has a chance to modify envs
    if args.dist == 'mac':
        from lib.build_mac import prepare, build
        envs = prepare(envs)
        build(envs)
    elif args.dist == 'dmg':
        from lib.build_mac import prepare
        envs = prepare(envs)

        from lib.build_dmg import prepare, build
        envs = prepare(envs)
        build(envs)
    elif args.dist == 'snap':
        from lib.build_snap import prepare, build
        envs = prepare(envs)
        build(envs)
    elif args.dist == 'nsis':
        from lib.build_nsis import prepare, build
        envs = prepare(envs)
        build(envs)

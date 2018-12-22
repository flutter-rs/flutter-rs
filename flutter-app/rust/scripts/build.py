#!/usr/bin/env python3
import os
import toml
import argparse
from lib import look_for_proj_dir, get_workspace_dir

def collect_env(args):
    PROJ_DIR = look_for_proj_dir(os.path.abspath(__file__))
    TOML_FILE = os.path.join(PROJ_DIR, 'Cargo.toml')
    META = toml.loads(open(TOML_FILE).read())
    NAME = META['package']['name']

    DEBUG = not args.release
    WORKSPACE = get_workspace_dir(PROJ_DIR)
    if WORKSPACE is not None:
        # cargo put outputs in workspace target directory
        TARGET_DIR = os.path.join(WORKSPACE, 'target')
    else:
        TARGET_DIR = os.path.join(PROJ_DIR, 'target')
    IDENTIFIER = 'one.juju.flutter-rs'
    FLUTTER_LIB_VER = META['package']['metadata']['flutter']['version']
    FLUTTER_ASSETS = os.path.join(os.path.dirname(PROJ_DIR), 'build', 'flutter_assets')
    return locals()

if __name__ == '__main__':
    parser = argparse.ArgumentParser(prog='build', description='rust app distribution builder')
    parser.add_argument('dist', choices=['mac', 'snap'], help='distribution type')
    parser.add_argument('--release', action='store_true', help='build release package')

    args = parser.parse_args()
    if args.dist == 'mac':
        from lib.build_mac import build
        build(collect_env(args))
    elif args.dist == 'snap':
        from lib.build_snap import build
        build(collect_env(args))

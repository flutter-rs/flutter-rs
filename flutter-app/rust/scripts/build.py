#!/usr/bin/env python3
import os
import toml
import argparse

def look_for_proj_dir(d):
    while not os.path.isfile(os.path.join(d, 'Cargo.toml')):
        p = os.path.dirname(d)
        if not p or p == d:
            return None
        d = p
    return d

def get_workspace_dir(proj_dir):
    return look_for_proj_dir(os.path.dirname(proj_dir))

def collect_env(args):
    PROJ_DIR = look_for_proj_dir(os.path.abspath(__file__))
    TOML_FILE = os.path.join(PROJ_DIR, 'Cargo.toml')
    META = toml.loads(open(TOML_FILE).read())
    NAME = META['package']['name']

    DEBUG = not args.release
    workspace = get_workspace_dir(PROJ_DIR)
    if workspace is not None:
        # cargo put outputs in workspace target directory
        TARGET_DIR = os.path.join(workspace, 'target')
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
        from build_mac import build
        build(collect_env(args))
    elif args.dist == 'snap':
        from build_snap import build
        build(collect_env(args))

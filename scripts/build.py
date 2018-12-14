#!/usr/bin/env python3
import os
import toml
import argparse

def look_for_proj_dir():
    d = os.getcwd()
    while not os.path.isfile(os.path.join(d, 'Cargo.toml')):
        p = os.path.dirname(d)
        if not p or p == d:
            print('Cannot find project directory')
        d = p
    return d

def collect_env(args):
    PROJ_DIR = look_for_proj_dir()
    TOML_FILE = os.path.join(PROJ_DIR, 'Cargo.toml')
    META = toml.loads(open(TOML_FILE).read())
    NAME = META['package']['name']

    DEBUG = not args.release
    TARGET_DIR = os.path.join(PROJ_DIR, 'target')
    APP_NAME = NAME + '.app'
    APP_DIR = os.path.join(TARGET_DIR, APP_NAME)
    IDENTIFIER = 'one.juju.flutter-rs'
    FLUTTER_LIB_VER = META['package']['metadata']['flutter']['version']
    FLUTTER_ASSETS = os.path.join(PROJ_DIR, 'examples', 'gallery', 'flutter_assets')
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

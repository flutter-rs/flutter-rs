#!/usr/bin/env python3
import os
import fnmatch
import subprocess
from string import Template
from lib import look_for_proj_dir

def get_config():
    try:
        proj_dir = look_for_proj_dir(os.path.abspath(__file__), 'pubspec.yaml')
        name = input('What\'s the name of the project?\n')
        lib_name = name.replace('-', '_')

        try:
            with open(os.path.join(proj_dir, '.initignore')) as f:
                ignore_list = []
                for line in f.readlines():
                    ignore = line.strip()

                    if os.path.isdir(os.path.join(proj_dir, ignore)):
                        ignore = os.path.join(ignore, '*')

                    ignore_list.append(ignore)
        except:
            ignore_list = []

        return {
            "name": name,
            "lib_name": lib_name, # underlined version of name
            "proj_dir": proj_dir,
            "ignore_list": ignore_list
        }
    except KeyboardInterrupt:
        return None


def install_py_deps(config):
    subprocess.run(
        ['pip3', 'install', '-r', './scripts/requirements.txt'],
        cwd = config['proj_dir'], check = True)

def tmpl_proj(config):
    proj_dir = config['proj_dir']
    for root, _, files in os.walk(proj_dir):
        for name in files:
            fp = os.path.join(root, name)
            fp_r = os.path.relpath(fp, proj_dir) # path relative to proj root
            ignored = False
            for ignore in config['ignore_list']:
                if fnmatch.fnmatch(fp_r, ignore):
                    ignored = True
                    break

            if not ignored:
                with open(fp, 'r+') as f:
                    s = Template(f.read()).substitute(**config)
                    f.seek(0)
                    f.truncate()
                    f.write(s)

def run():
    config = get_config()
    if not config:
        return

    # if a name is not specified, skip templating process
    if config['name']:
        print('>>> Creating files')
        tmpl_proj(config)

    print('>>> Installing build dependencies')
    install_py_deps(config)

    print('>>> Done! Happy coding.')

if __name__ == '__main__':
    run()

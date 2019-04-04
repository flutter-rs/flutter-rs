#!/usr/bin/env python3
import os
import glob
import subprocess
from string import Template
from lib import look_for_proj_dir

def get_config():
    try:
        proj_dir = look_for_proj_dir(os.path.abspath(__file__), 'pubspec.yaml')
        name = input('What\'s the name of the project?\n')
        lib_name = name.replace('-', '_')

        tmplfile_path = os.path.join(proj_dir, '.tmplfiles')
        try:
            with open(tmplfile_path) as f:
                tmplfiles = []
                for line in f.readlines():
                    line = line.strip()

                    if os.path.isdir(os.path.join(proj_dir, line)):
                        line = os.path.join(line, '*')

                    tmplfiles.append(line)
        except:
            tmplfiles = []

        return {
            "name": name,
            "lib_name": lib_name, # underlined version of name
            "proj_dir": proj_dir,
            "tmplfiles": tmplfiles,
            "tmplfile_path": tmplfile_path
        }
    except KeyboardInterrupt:
        return None


def install_py_deps(config):
    subprocess.run(
        ['pip3', 'install', '-r', './scripts/requirements.txt'],
        cwd = config['proj_dir'], check = True)

def tmpl_proj(config):
    proj_dir = config['proj_dir']
    for pattern in config['tmplfiles']:
        for fp in glob.iglob(os.path.join(proj_dir, pattern)):
            fp = os.path.join(proj_dir, fp)
            if os.path.isfile(fp):
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

    # remove tmplfile, useless now
    os.remove(config['tmplfile_path'])

if __name__ == '__main__':
    run()

import os, sys, subprocess

def look_for_proj_dir(d, fn = 'Cargo.toml'):
    while not os.path.isfile(os.path.join(d, fn)):
        p = os.path.dirname(d)
        if not p or p == d:
            return None
        d = p
    return d

def get_workspace_dir(proj_dir):
    return look_for_proj_dir(os.path.dirname(proj_dir))

def read_sdk_version(root):
    fp = os.path.join(root, 'bin', 'internal', 'engine.version')
    with open(fp) as f:
        return f.read().strip()

def guess_sdk_path():
    try:
        output = subprocess.check_output([
            'where.exe' if sys.platform == 'win32' else 'which',
            'flutter'
        ], encoding = 'utf8')
        lines = output.strip().split()
        path = lines[0]
        path = os.path.dirname(path)
        path = os.path.dirname(path)
        return path
    except (FileNotFoundError, subprocess.CalledProcessError):
        pass

# Read engine version from FLUTTER_ROOT first
# otherwise use FLUTTER_ENGINE_VERSION env variable
def get_flutter_version():
    if 'FLUTTER_ENGINE_VERSION' in os.environ:
        return os.environ.get('FLUTTER_ENGINE_VERSION')

    root = os.environ.get('FLUTTER_ROOT')
    if root:
        return read_sdk_version(root)
    else:
        sdk = guess_sdk_path()
        print("sdk is proballby at", sdk)
        if sdk:
            return read_sdk_version(sdk)
        else:
            raise Exception('Cannot find flutter engine version. flutter cli not in PATH. You may need to set either FLUTTER_ROOT or FLUTTER_ENGINE_VERSION')

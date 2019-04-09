import os

def look_for_proj_dir(d, fn = 'Cargo.toml'):
    while not os.path.isfile(os.path.join(d, fn)):
        p = os.path.dirname(d)
        if not p or p == d:
            return None
        d = p
    return d

def get_workspace_dir(proj_dir):
    return look_for_proj_dir(os.path.dirname(proj_dir))

# Read engine version from FLUTTER_ROOT first
# otherwise use FLUTTER_ENGINE_VERSION env variable
def get_flutter_version():
    if 'FLUTTER_ENGINE_VERSION' in os.environ:
        return os.environ.get('FLUTTER_ENGINE_VERSION')
    root = os.environ.get('FLUTTER_ROOT')
    if root:
        fp = os.path.join(root, 'bin', 'internal', 'engine.version')
        with open(fp) as f:
            return f.read().strip()
    else:
        raise Exception('Cannot get flutter engine version. Either FLUTTER_ROOT or FLUTTER_ENGINE_VERSION has to be set"')

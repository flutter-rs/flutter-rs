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

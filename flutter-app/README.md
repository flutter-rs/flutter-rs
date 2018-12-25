# flutter-app

A desktop app built using flutter & rust.

![screenshot](https://raw.githubusercontent.com/gliheng/flutter-rs/master/www/images/screenshot_mac.png)


- Runs on mac and linux, so far.
- Build distribution only works on mac.
- Windows will be supported.

# Requirement

- [Rust](https://www.rust-lang.org/tools/install)
- libglfw:
    - Install on Mac with: `brew install glfw`
    - Install on linux with `apt install libglfw3`

# Workflow
- To develop with hot-reloading, use:
    `./scripts/run.py`

- To build distribution, use:
    `./scripts/build.py mac`

**Note:**
Build scripts are written in python3. Install python depenendencies using `pip3 install -r scripts/requirements.txt`

---

### For users in China
Please ensure you have access to *storage.googleapis.com*. It is required to download lib_flutter. 

Set appropriate http proxy in the terminal by using:
```shell
export http_proxy=...
export https_proxy=...
```
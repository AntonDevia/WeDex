"""Build the self-contained Linux server embedded by the Windows VPS installer.

Windows: rustup target add x86_64-unknown-linux-musl; cargo install cargo-zigbuild --locked
Put a Zig executable on PATH (or supply --zig-dir). Linux can use musl-gcc directly.
"""
from pathlib import Path
import argparse,os,subprocess,shutil,hashlib,json
ROOT=Path(__file__).resolve().parents[1]
def main():
    parser=argparse.ArgumentParser();parser.add_argument('--zig-dir');args=parser.parse_args()
    env=os.environ.copy()
    cargo=Path.home()/'.cargo/bin'/('cargo.exe' if os.name=='nt' else 'cargo')
    if args.zig_dir:env['PATH']=str(Path(args.zig_dir).resolve())+os.pathsep+env.get('PATH','')
    command='zigbuild' if os.name=='nt' or args.zig_dir else 'build'
    subprocess.run([str(cargo),command,'-p','wedex-cloud','--bin','wedex-server','--target','x86_64-unknown-linux-musl','--release','--locked'],cwd=ROOT,env=env,check=True)
    source=ROOT/'target/x86_64-unknown-linux-musl/release/wedex-server'
    data=source.read_bytes()
    assert data[:6]==b'\x7fELF\x02\x01' and data[18:20]==b'\x3e\0','Unexpected Linux executable'
    dest=ROOT/'assets/server/wedex-server-x86_64-linux-musl';dest.parent.mkdir(exist_ok=True)
    shutil.copy2(source,dest)
    print(json.dumps({'binary':str(dest),'bytes':len(data),'sha256':hashlib.sha256(data).hexdigest()}))
if __name__=='__main__':main()

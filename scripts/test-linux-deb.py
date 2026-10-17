#!/usr/bin/env python3
"""Negative integration checks against a generated package; never installs it."""
import importlib.util,json,os,re,shutil,subprocess,sys,tempfile
from pathlib import Path
sys.dont_write_bytecode = True
script=Path(__file__).resolve().with_name('linux_deb.py')
package=Path(sys.argv[1]).resolve()
spec=importlib.util.spec_from_file_location('linux_deb',script)
module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
assert module.covers(['libc6 (>= 2.43)'],'libc6 (>= 2.39)')
assert not module.covers(['libc6'],'libc6 (>= 2.39)')
assert not module.covers(['libc6 (>= 2.17)'],'libc6 (>= 2.39)')
checks=[]
with tempfile.TemporaryDirectory(prefix='asa-negative-deb-') as directory:
    base=Path(directory)
    subprocess.run([sys.executable,str(script),'verify',str(package)],check=True,stdout=subprocess.DEVNULL)
    checks.append('correct package passes')
    for name,replacement in [('unversioned','libc6'),('too-old','libc6 (>= 2.17)'),('removed','')]:
        work=base/name
        subprocess.run(['dpkg-deb','-R',str(package),str(work)],check=True)
        control=work/'DEBIAN/control';text=control.read_text()
        text=re.sub(r'libc6 \(>= [^)]*\)(?:, )?',replacement+(', ' if replacement else ''),text)
        control.write_text(text)
        candidate=base/(name+'.deb')
        subprocess.run(['dpkg-deb','--build',str(work),str(candidate)],check=True,stdout=subprocess.DEVNULL)
        result=subprocess.run([sys.executable,str(script),'verify',str(candidate)],capture_output=True,text=True)
        assert result.returncode!=0,(name,result.stdout)
        assert 'libc6' in result.stderr,result.stderr
        checks.append(name+' libc6 constraint is rejected')
    for name in ['missing-entry','missing-gsettings-dependency','invalid-desktop-entry','unsafe-postinst']:
        work=base/name
        subprocess.run(['dpkg-deb','-R',str(package),str(work)],check=True)
        if name=='missing-entry':(work/'usr/bin/ai-selection-assistant').unlink()
        elif name=='missing-gsettings-dependency':
            control=work/'DEBIAN/control';control.write_text(control.read_text().replace('libglib2.0-bin, ',''))
        elif name=='invalid-desktop-entry':
            entry=work/'usr/share/applications/AI Selection Assistant.desktop'
            entry.write_text(re.sub(r'^Exec=.*$','Exec=/nonexistent',entry.read_text(),flags=re.M))
        else:
            entry=work/'DEBIAN/postinst';entry.write_text('#!/bin/sh\ngsettings set example value true\n');entry.chmod(0o755)
        candidate=base/(name+'.deb')
        subprocess.run(['dpkg-deb','--build',str(work),str(candidate)],check=True,stdout=subprocess.DEVNULL)
        result=subprocess.run([sys.executable,str(script),'verify',str(candidate)],capture_output=True,text=True)
        assert result.returncode!=0,(name,result.stdout)
        checks.append(name+' is rejected')
    result=subprocess.run([sys.executable,str(script),'config',str(package)],env={**os.environ,'PATH':'/nonexistent'},capture_output=True,text=True)
    assert result.returncode!=0 and 'missing' in result.stderr
    checks.append('missing tool fails closed')
    for size in [module.MAX_DEB_BYTES, module.MAX_DEB_BYTES+1]:
        oversized=base/f'oversized-{size}.deb'
        with oversized.open('wb') as stream: stream.truncate(size)
        result=subprocess.run([sys.executable,str(script),'verify',str(oversized)],capture_output=True,text=True)
        assert result.returncode!=0 and 'size budget exceeded' in result.stderr
        checks.append(f'{size} byte package is rejected')
    fake=base/'fake';fake.mkdir()
    for tool in ['dpkg-deb','dpkg','readelf']:(fake/tool).symlink_to(shutil.which(tool))
    shim=fake/'dpkg-shlibdeps';shim.write_text('#!/bin/sh\necho "shlibs:Depends="\n');shim.chmod(0o755)
    result=subprocess.run([sys.executable,str(script),'config',str(package)],env={**os.environ,'PATH':str(fake)},capture_output=True,text=True)
    assert result.returncode!=0 and 'metadata' in result.stderr
    checks.append('missing metadata fails closed')
print(json.dumps({'passed':len(checks),'checks':checks},indent=2))

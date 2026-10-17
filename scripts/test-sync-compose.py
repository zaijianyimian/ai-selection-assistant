#!/usr/bin/env python3
"""Execute documented Compose administration commands in a uniquely named local project."""
import argparse,json,subprocess,tempfile,time,uuid
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--output',required=True);a=p.parse_args();out=Path(a.output);out.mkdir(parents=True,exist_ok=True);root=Path(__file__).resolve().parents[1];project='asaqa'+uuid.uuid4().hex[:10]
with tempfile.TemporaryDirectory()as tmp:
 override=Path(tmp)/'override.yaml';override.write_text('services:\n  sync:\n    image: asa-sync-server:qa\n')
 base=['docker','compose','--env-file',str(root/'deploy/.env.example'),'-f',str(root/'deploy/compose.yaml'),'-f',str(override),'-p',project]
 def run(*args,input=None):
  r=subprocess.run([*base,*args],input=input,text=True,capture_output=True);assert r.returncode==0,(args,r.stderr);return r.stdout
 try:
  run('config','--quiet');run('run','--rm','-T','--no-deps','sync','init',input='fixture-compose-password-only');run('up','-d','--no-build','sync')
  assert run('exec','-T','sync','id','-u').strip()=='10001';run('exec','-T','sync','asa-sync-server','backup','/backups/compose.sqlite3')
  times=[]
  for i in range(3):
   start=time.perf_counter();run('exec','-T','sync','asa-sync-server','rotate-password',input='fixture-compose-rotated-password-only');times.append(time.perf_counter()-start)
  peak=run('exec','-T','sync','cat','/sys/fs/cgroup/memory.peak').strip()
  run('stop','sync');run('run','--rm','--no-deps','sync','restore','/backups/compose.sqlite3');run('up','-d','--no-build','sync');run('exec','-T','sync','asa-sync-server','status')
  (out/'compose-result.json').write_text(json.dumps({'passed':True,'checks':['actual Compose configuration','non-root init through stdin','persistent volumes','online consistent backup','password rotation through stdin','stopped service restore','service restart'],'argon2':{'memoryKiB':65536,'iterations':3,'parallelism':1,'cliSamplesSeconds':times,'containerPeakBytes':int(peak)if peak.isdigit()else peak}},indent=2));print('Compose rehearsal passed')
 finally:run('down','--volumes','--remove-orphans')

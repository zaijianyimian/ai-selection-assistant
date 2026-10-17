#!/usr/bin/env python3
"""Disposable Caddy internal-CA rehearsal; no public DNS/certificate issuance."""
import argparse,json,ssl,subprocess,tempfile,time,urllib.request,uuid
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--output',required=True);a=p.parse_args();out=Path(a.output);out.mkdir(parents=True,exist_ok=True)
name='asa-tls-qa-'+uuid.uuid4().hex[:8];network=name+'-net';volume=name+'-data';checks=[]
def run(*args,ok=True):
 r=subprocess.run(['docker',*args],capture_output=True,text=True);assert not ok or r.returncode==0,(args,r.stderr);return r
try:
 run('network','create',network);run('volume','create',volume)
 run('run','--rm','-v',volume+':/data','--entrypoint','sh','asa-sync-server:qa','-c',"printf '%s' fixture-tls-password-only | asa-sync-server init")
 run('run','-d','--name',name+'-sync','--network',network,'--network-alias','sync','-v',volume+':/data','asa-sync-server:qa')
 with tempfile.TemporaryDirectory()as tmp:
  tmp=Path(tmp);config=tmp/'Caddyfile';config.write_text('https://localhost {\n tls internal\n reverse_proxy sync:8080\n}\n')
  run('run','-d','--name',name+'-caddy','--network',network,'-p','127.0.0.1::443','-v',str(config)+':/etc/caddy/Caddyfile:ro','caddy:2.10-alpine')
  for _ in range(100):
   r=run('cp',name+'-caddy:/data/caddy/pki/authorities/local/root.crt',str(tmp/'root.crt'),ok=False)
   if r.returncode==0:break
   time.sleep(.1)
  port=json.loads(run('inspect',name+'-caddy').stdout)[0]['NetworkSettings']['Ports']['443/tcp'][0]['HostPort'];url='https://localhost:'+port+'/healthz'
  context=ssl.create_default_context(cafile=str(tmp/'root.crt'))
  for _ in range(100):
   try:
    with urllib.request.urlopen(url,context=context,timeout=2)as response:
     assert json.load(response)=={'status':'ok'};break
   except OSError:time.sleep(.1)
  else:raise AssertionError('TLS proxy did not become ready')
  checks.append('TLS Caddy proxy reaches private service with trusted local CA')
  try:urllib.request.urlopen(url,timeout=3);raise AssertionError('untrusted certificate accepted')
  except urllib.error.URLError as e:assert isinstance(e.reason,ssl.SSLCertVerificationError)
  checks.append('untrusted TLS CA rejected by default verifier')
  ports=json.loads(run('inspect',name+'-sync').stdout)[0]['NetworkSettings']['Ports'];assert not any(v for v in ports.values());checks.append('service publishes no host ports')
 (out/'tls-result.json').write_text(json.dumps({'passed':len(checks),'checks':checks},indent=2));print('\n'.join('PASS '+c for c in checks))
finally:
 run('rm','-f',name+'-sync',name+'-caddy',ok=False);run('network','rm',network,ok=False);run('volume','rm',volume,ok=False)

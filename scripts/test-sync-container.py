#!/usr/bin/env python3
"""Isolated, disposable service rehearsal. Only resources named by this run are removed."""
import argparse,json,subprocess,tempfile,time,urllib.request,urllib.error,uuid
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--image',default='asa-sync-server:qa');p.add_argument('--output',required=True);a=p.parse_args();out=Path(a.output);out.mkdir(parents=True,exist_ok=True)
name='asa-sync-qa-'+uuid.uuid4().hex[:10];volumes=[name+'-data',name+'-backups'];checks=[]
def run(*args,input=None,ok=True):
 r=subprocess.run(['docker',*args],input=input,text=True,capture_output=True);assert not ok or r.returncode==0,(args,r.stderr);return r
mounts=['-v',volumes[0]+':/data','-v',volumes[1]+':/backups']
def admin(*args,input=None,ok=True):return run('run','--rm','-i',*mounts,a.image,*args,input=input,ok=ok)
def check(name,value):assert value,name;checks.append(name);print('PASS',name,flush=True)
def ready():
 global url
 port=json.loads(run('inspect',name).stdout)[0]['NetworkSettings']['Ports']['8080/tcp'][0]['HostPort'];url='http://127.0.0.1:'+port
 for _ in range(100):
  try:
   if request('GET','/healthz')[0]==200:return
  except OSError:time.sleep(.1)
 raise AssertionError('container did not become ready')
def request(method,path,data=None,token=None):
 headers={'Content-Type':'application/json'}
 if token:headers['Authorization']='Bearer '+token
 req=urllib.request.Request(url+path,data=None if data is None else json.dumps(data).encode(),method=method,headers=headers)
 try:
  with urllib.request.urlopen(req,timeout=10)as r:return r.status,json.load(r)if r.status!=204 else None
 except urllib.error.HTTPError as e:return e.code,json.load(e)
try:
 for v in volumes:run('volume','create','--label','asa.sync.qa=true',v)
 password='fixture-container-password-only';admin('init',input=password)
 run('run','-d','--name',name,'--label','asa.sync.qa=true','--read-only','--tmpfs','/tmp:rw,noexec,nosuid,size=16m','--cap-drop','ALL','--security-opt','no-new-privileges:true','-p','127.0.0.1::8080',*mounts,a.image)
 port=json.loads(run('inspect',name).stdout)[0]['NetworkSettings']['Ports']['8080/tcp'][0]['HostPort'];url='http://127.0.0.1:'+port
 for _ in range(50):
  try:
   if request('GET','/healthz')[0]==200:break
  except OSError:time.sleep(.1)
 check('non-root UID 10001 and private database',run('exec',name,'id','-u').stdout.strip()=='10001' and run('exec',name,'stat','-c','%a','/data/sync.sqlite3').stdout.strip()=='600')
 login={'protocol':1,'password':password,'deviceName':'container-fixture'};status,c=request('POST','/v1/auth/check',login);check('connection check authenticates',status==200)
 status,e=request('POST','/v1/devices/enroll',login);token=e['token'];context=e['context'];check('enrollment succeeds',status==200)
 payload={'protocol':1,'workspaceId':context['workspaceId'],'historyId':context['historyId'],'operations':[{'id':'fixture-pref','generation':1,'action':{'type':'preference','field':'theme','baseRevision':0,'value':'dark'}}]}
 check('authenticated change committed',request('POST','/v1/push',payload,token)[0]==200)
 run('restart',name);ready();check('restart preserves token and accepted revision',request('GET','/v1/changes?stream=preferences&after=0',token=token)[1]['changes'][0]['change']['value']=='dark')
 run('exec',name,'asa-sync-server','backup','/backups/snapshot.sqlite3');check('backup is private',run('exec',name,'stat','-c','%a','/backups/snapshot.sqlite3').stdout.strip()=='600')
 check('live restore blocked by process lock',run('exec',name,'asa-sync-server','restore','/backups/snapshot.sqlite3',ok=False).returncode!=0)
 run('exec','-i',name,'asa-sync-server','rotate-password',input='fixture-rotated-password-only');check('rotation revokes token',request('GET','/v1/devices',token=token)[0]==401)
 run('stop',name);admin('restore','/backups/snapshot.sqlite3');run('start',name);ready()
 status,new=request('POST','/v1/auth/check',login);check('restore changes history but retains workspace',status==200 and new['historyId']!=context['historyId']and new['workspaceId']==context['workspaceId'])
 check('restore invalidates old token',request('GET','/v1/devices',token=token)[0]==401)
 status,e2=request('POST','/v1/devices/enroll',login);check('restore preserves accepted data',request('GET','/v1/changes?stream=preferences&after=0',token=e2['token'])[1]['changes'][0]['change']['value']=='dark')
 logs=run('logs',name).stdout;check('logs omit fixture secrets and data',all(x not in logs for x in [password,token,e2['token'],'fixture-pref','"dark"']))
 (out/'container-result.json').write_text(json.dumps({'passed':len(checks),'checks':checks,'image':a.image},ensure_ascii=False,indent=2))
finally:
 run('rm','-f',name,ok=False)
 for v in volumes:run('volume','rm',v,ok=False)

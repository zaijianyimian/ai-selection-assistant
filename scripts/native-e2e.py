#!/usr/bin/env python3
"""Real Tauri IPC + WebKit UI test. Run ONLY under isolated Xvfb and dbus-run-session.
Needs tauri-driver, WebKitWebDriver and gnome-keyring-daemon. Never uses user credentials.
"""
import argparse, base64, ctypes, http.server, json, os, signal, subprocess, tempfile, threading, time, urllib.request
from pathlib import Path

parser=argparse.ArgumentParser()
parser.add_argument('--binary',required=True)
parser.add_argument('--driver',required=True)
parser.add_argument('--native-driver',required=True)
parser.add_argument('--output',required=True)
a=parser.parse_args()
assert not os.environ.get('WAYLAND_DISPLAY'), 'Use an isolated Xvfb display'
assert os.environ.get('AI_SELECTION_E2E_ISOLATED')=='1', 'Explicit isolation is required'
out=Path(a.output).resolve();out.mkdir(parents=True,exist_ok=True)
results=[]
requests=[]
delay=threading.Event()
class Provider(http.server.BaseHTTPRequestHandler):
    def do_POST(self):
        body=json.loads(self.rfile.read(int(self.headers['Content-Length'])))
        requests.append(body)
        if body['messages'][0]['content'].startswith('Analyze technical'):
            content={'topics':[{'domain':'Agent','evidenceIds':[1]}]}
        else:
            if 'slow' in body['messages'][1]['content']: delay.wait(10)
            content={'translation':'智能体完成任务。','sourceLanguage':'English','terms':[{'original':'Agent','translation':'智能体','domain':'Agent','explanation':'自主执行任务的系统','senseSummary':'自主执行任务的系统','example':None}]}
        data=json.dumps({'choices':[{'message':{'content':json.dumps(content)},'finish_reason':'stop'}]}).encode()
        self.send_response(200); self.send_header('Content-Type','application/json'); self.send_header('Content-Length',str(len(data)));self.end_headers();self.wfile.write(data)
    def log_message(self,*args): pass
server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Provider)
threading.Thread(target=server.serve_forever,daemon=True).start()

def wait(check,timeout=20):
    deadline=time.time()+timeout
    while time.time()<deadline:
        try:
            value=check()
            if value: return value
        except (OSError,ValueError,RuntimeError): pass
        time.sleep(.15)
    raise AssertionError('Timed out: '+str(check))

def http(method,path,data=None):
    req=urllib.request.Request('http://127.0.0.1:4444'+path,data=None if data is None else json.dumps(data).encode(),method=method,headers={'Content-Type':'application/json'})
    try:
        with urllib.request.urlopen(req,timeout=35) as response: value=json.load(response)['value']
    except urllib.error.HTTPError as e:
        raise RuntimeError(e.read().decode()) from e
    if isinstance(value,dict) and isinstance(value.get('error'),str): raise RuntimeError(str(value))
    return value

session=None;driver=None;keyring=None
with tempfile.TemporaryDirectory(prefix='asa-native-e2e-') as directory:
    base=Path(directory)
    env=os.environ.copy()
    for key,sub in [('XDG_CONFIG_HOME','config'),('XDG_DATA_HOME','data'),('XDG_CACHE_HOME','cache'),('XDG_RUNTIME_DIR','runtime')]:
        path=base/sub;path.mkdir(mode=0o700);env[key]=str(path)
    env.pop('GNOME_KEYRING_CONTROL',None)
    env['WEBKIT_DISABLE_COMPOSITING_MODE']='1'
    config_path=base/'config/io.github.ai-selection-assistant/settings.json';config_path.parent.mkdir(parents=True)
    config_path.write_text(json.dumps({'baseUrl':f'http://127.0.0.1:{server.server_port}/v1','model':'fixture-model','rememberTerms':True}))
    try:
        keyring=subprocess.run(['gnome-keyring-daemon','--unlock','--components=secrets'],input='fixture-keyring-password\n',text=True,capture_output=True,env=env,timeout=10)
        assert keyring.returncode==0,keyring.stderr
        for line in keyring.stdout.splitlines():
            if '=' in line:
                k,v=line.split('=',1)
                if k.startswith('GNOME_KEYRING'):env[k]=v
        with (out/'driver.log').open('w') as log:
            driver=subprocess.Popen([str(Path(a.driver).resolve()),'--native-driver',str(Path(a.native_driver).resolve())],env=env,stdout=log,stderr=log,start_new_session=True)
            wait(lambda:http('GET','/status'))
            session=http('POST','/session',{'capabilities':{'alwaysMatch':{'browserName':'wry','tauri:options':{'application':str(Path(a.binary).resolve())}}}})['sessionId']
            prefix='/session/'+session
            def js(script,*args):return http('POST',prefix+'/execute/sync',{'script':script,'args':list(args)})
            def invoke(name,args=None,allow_error=False):
                value=http('POST',prefix+'/execute/async',{'script':"const done=arguments[arguments.length-1]; window.__TAURI_INTERNALS__.invoke(arguments[0],arguments[1]).then(x=>done({ok:x}),e=>done({error:e}));",'args':[name,args or {}]})
                if 'error' in value and not allow_error:raise AssertionError((name,value['error']))
                return value.get('ok') if 'ok' in value else value
            def body():return js('return document.body.innerText')
            def check(name,condition):
                assert condition,name
                results.append(name); print('PASS',name,flush=True)
            wait(lambda:'工作空间' in body())
            current=invoke('get_settings')['settings']
            invoke('save_settings',{'config':current,'apiKey':'fixture-provider-key','removeApiKey':False})
            check('isolated credential save/read',invoke('get_settings')['hasApiKey'])
            invoke('translate_text',{'text':'An Agent completes a task.'})
            wait(lambda:invoke('get_job')['status']=='success')
            invoke('analyze_memory')
            wait(lambda:not invoke('get_analysis_status')['running'])
            memory=invoke('get_memory',{'query':'','offset':0})
            check('real translation and profile persisted',len(memory['versions'])==1 and memory['total']==1)
            # Reserve Alt+Shift+T in another X11 client so the actual native register fails.
            x=ctypes.CDLL('libX11.so.6');x.XOpenDisplay.restype=ctypes.c_void_p
            x.XDefaultRootWindow.argtypes=[ctypes.c_void_p];x.XDefaultRootWindow.restype=ctypes.c_ulong
            x.XKeysymToKeycode.argtypes=[ctypes.c_void_p,ctypes.c_ulong];x.XKeysymToKeycode.restype=ctypes.c_uint
            x.XGrabKey.argtypes=[ctypes.c_void_p,ctypes.c_int,ctypes.c_uint,ctypes.c_ulong,ctypes.c_int,ctypes.c_int,ctypes.c_int]
            x.XUngrabKey.argtypes=[ctypes.c_void_p,ctypes.c_int,ctypes.c_uint,ctypes.c_ulong]
            x.XSync.argtypes=[ctypes.c_void_p,ctypes.c_int];x.XCloseDisplay.argtypes=[ctypes.c_void_p]
            display=x.XOpenDisplay(None);root=x.XDefaultRootWindow(display);key=x.XKeysymToKeycode(display,ord('T'))
            x.XGrabKey(display,key,9,root,1,1,1);x.XSync(display,0)
            rejected=invoke('save_settings',{'config':{**current,'rememberTerms':False,'shortcut':'Alt+Shift+T'},'apiKey':None,'removeApiKey':False},True)
            x.XUngrabKey(display,key,9,root);x.XSync(display,0);x.XCloseDisplay(display)
            check('native shortcut conflict reports save failure','error' in rejected)
            check('failed save preserves profile and memory',invoke('get_memory',{'query':'','offset':0})==memory)
            check('failed save preserves switches',invoke('get_settings')['settings']['rememberTerms'])
            # Exercise actual frontend draft watcher against real settings-changed IPC.
            invoke('show_settings');wait(lambda:js("return !!document.querySelector('#model')"))
            js("const e=document.querySelector('#model');e.value='unsaved-draft';e.dispatchEvent(new Event('input',{bubbles:true}));")
            external={**current,'model':'external-model'}
            invoke('save_settings',{'config':external,'apiKey':None,'removeApiKey':False})
            wait(lambda:'设置已在其他窗口更新' in body())
            check('native UI preserves draft',js("return document.querySelector('#model').value")=='unsaved-draft')
            js("document.querySelector('[data-testid=\"settings-external-update\"] button').click()")
            wait(lambda:js("return document.querySelector('#model').value")=='external-model')
            check('native UI reloads saved configuration',True)
            # Run a real asynchronous request, then update settings before releasing the local provider.
            invoke('translate_text',{'text':'A slow Agent completes a task.'})
            wait(lambda:len(requests)>=3 and 'slow' in requests[-1]['messages'][1]['content'])
            invoke('save_settings',{'config':{**external,'targetLanguage':'日本語','explainTerms':False},'apiKey':None,'removeApiKey':False})
            delay.set();wait(lambda:invoke('get_job')['status']=='success')
            check('in-flight request retains native options',invoke('get_job')['requestOptions']=={'targetLanguage':'简体中文','explainTerms':True})
            js("Array.from(document.querySelectorAll('nav button')).find(e=>e.textContent.includes('划词翻译')).click()")
            wait(lambda:js("return document.querySelector('.language-tag')?.textContent")=='简体中文')
            check('main window renders request language',True)
            invoke('show_popup')
            handles=http('GET',prefix+'/window/handles')
            popup_found=False
            for handle in handles:
                http('POST',prefix+'/window',{'handle':handle})
                if js('return location.search').find('translation')>=0:
                    wait(lambda:js("return document.querySelector('.language-tag')?.textContent")=='简体中文')
                    popup_found=True
                    break
            check('popup restores same result and language',popup_found)
            # Recovery gate uses only a non-secret file; it blocks all three model entry paths.
            config_path.with_suffix('.recovery').write_text('settings recovery required\n')
            count=len(requests)
            for name,args in [('translate_text',{'text':'Agent'}),('test_connection',{'config':external,'apiKey':'fixture-provider-key'}),('analyze_memory',{})]:
                check('recovery gate blocks '+name,'error' in invoke(name,args,True))
            check('recovery gate issued no provider requests',len(requests)==count)
            invoke('save_settings',{'config':external,'apiKey':'fixture-provider-key','removeApiKey':False})
            check('successful explicit recovery clears marker',not config_path.with_suffix('.recovery').exists())
            check('recovery preserves existing terms',invoke('get_memory',{'query':'','offset':0})['total']==1)
            screenshot=http('GET',prefix+'/screenshot')
            (out/'native-result.png').write_bytes(base64.b64decode(screenshot))
            (out/'result.json').write_text(json.dumps({'passed':len(results),'checks':results,'providerRequests':len(requests)},ensure_ascii=False,indent=2))
    finally:
        delay.set()
        if session:
            try:http('DELETE','/session/'+session)
            except Exception:pass
        if driver and driver.poll() is None:
            os.killpg(driver.pid,signal.SIGTERM)
            driver.wait(timeout=5)
        server.shutdown()

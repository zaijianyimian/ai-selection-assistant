#!/usr/bin/env python3
"""Real Tauri IPC + WebKit UI test. Run ONLY under isolated Xvfb and dbus-run-session.
Needs tauri-driver, WebKitWebDriver, xclip, Openbox, gnome-keyring-daemon, Python GI and Pillow. Never uses user credentials.
"""
import argparse, base64, ctypes, http.server, json, os, re, signal, subprocess, tempfile, threading, time, urllib.request, socket
from pathlib import Path

parser=argparse.ArgumentParser()
parser.add_argument('--binary',required=True)
parser.add_argument('--driver',required=True)
parser.add_argument('--native-driver',required=True)
parser.add_argument('--output',required=True)
parser.add_argument('--xclip',required=True)
parser.add_argument('--sync-server',help='Optional independent sync service binary for native sync IPC tests')
a=parser.parse_args()
assert not os.environ.get('WAYLAND_DISPLAY'), 'Use an isolated Xvfb display'
assert os.environ.get('AI_SELECTION_E2E_ISOLATED')=='1', 'Explicit isolation is required'
out=Path(a.output).resolve();out.mkdir(parents=True,exist_ok=True)
results=[]
requests=[]
delay=threading.Event()
usage_delay=threading.Event()
class Provider(http.server.BaseHTTPRequestHandler):
    def do_POST(self):
        body=json.loads(self.rfile.read(int(self.headers['Content-Length'])))
        requests.append(body)
        text=body['messages'][1]['content']
        if text=='usage-retry-fixture' and body.get('response_format',{}).get('type')=='json_schema':
            data=json.dumps({'error':'response_format json_schema unsupported','usage':{'prompt_tokens':5,'completion_tokens':0,'total_tokens':5}}).encode()
            self.send_response(400);self.send_header('Content-Length',str(len(data)));self.end_headers();self.wfile.write(data);return
        if text=='usage-hold-fixture':usage_delay.wait(15)
        if body['messages'][1]['content']=='cache-error-fixture':
            self.send_response(500);self.end_headers();self.wfile.write(b'{"error":{"message":"fixture server failure"}}');return
        if body['messages'][0]['content'].startswith('Analyze technical'):
            content={'topics':[{'domain':'Agent','evidenceIds':[1]}]}
        else:
            if 'slow' in body['messages'][1]['content']: delay.wait(10)
            content={'translation':('智能体完成任务。'*50 if 'slow' in body['messages'][1]['content'] else '智能体完成任务。'),'sourceLanguage':'English','terms':[{'original':'Agent','translation':'智能体','domain':'Agent','explanation':'自主执行任务的系统','senseSummary':'自主执行任务的系统','example':None}]}
        if text.startswith('usage-'):content['terms']=[]
        envelope={'choices':[{'message':{'content':json.dumps(content)},'finish_reason':'stop'}], 'usage':{'prompt_tokens':100,'completion_tokens':30,'total_tokens':130,'prompt_tokens_details':{'cached_tokens':20},'completion_tokens_details':{'reasoning_tokens':10}}}
        if text=='usage-unknown-fixture':envelope.pop('usage')
        if text=='usage-truncated-fixture':envelope['choices'][0]['finish_reason']='length'
        if text=='usage-refused-fixture':envelope['choices'][0]['message']['refusal']='fixture refusal'
        if text=='usage-bad-json-fixture' or body['model']=='usage-invalid-model':envelope['choices'][0]['message']['content']='not valid JSON private-source-marker'
        data=json.dumps(envelope).encode()
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

session=None;driver=None;keyring=None;selection_owner=None;sync_process=None;tray_process=None;clipboard_owner=None;reserved_display=None
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
        if a.sync_server:
            sync_env={**env,'SYNC_DATABASE':str(base/'sync-service.sqlite3')}
            with socket.socket() as port_socket:
                port_socket.bind(('127.0.0.1',0));sync_port=port_socket.getsockname()[1]
            sync_env['SYNC_BIND']=f'127.0.0.1:{sync_port}'
            sync_binary=str(Path(a.sync_server).resolve())
            subprocess.run([sync_binary,'init'],input='fixture-native-sync-password-only',text=True,env=sync_env,capture_output=True,check=True)
            sync_log=(out/'sync-service.log').open('w')
            sync_process=subprocess.Popen([sync_binary,'serve'],env=sync_env,stdout=sync_log,stderr=sync_log)
            sync_url=f'http://127.0.0.1:{sync_port}'
            def service_ready():
                with urllib.request.urlopen(sync_url+'/healthz',timeout=2) as r:return r.status==200
            wait(service_ready)
        keyring=subprocess.run(['gnome-keyring-daemon','--unlock','--components=secrets'],input='fixture-keyring-password\n',text=True,capture_output=True,env=env,timeout=10)
        assert keyring.returncode==0,keyring.stderr
        for line in keyring.stdout.splitlines():
            if '=' in line:
                k,v=line.split('=',1)
                if k.startswith('GNOME_KEYRING'):env[k]=v
        tray_process=subprocess.Popen(['python3',str(Path(__file__).with_name('native-tray-watch.py')),str(out/'tray-icon.json'),str(Path(__file__).resolve().parents[1]/'src-tauri/icons/32x32.png')],env=env,stdout=(out/'tray-watch.log').open('w'),stderr=subprocess.STDOUT)
        wm=subprocess.Popen(['openbox'],env=env,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
        time.sleep(.5)
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
            shortcut_status=invoke('get_shortcut_status')
            check('native shortcut ready requires real registration',shortcut_status['backend']=='native' and shortcut_status['status']=='ready')
            current=invoke('get_settings')['settings']
            invoke('save_settings',{'config':current,'apiKey':'fixture-provider-key','removeApiKey':False})
            check('isolated credential save/read',invoke('get_settings')['hasApiKey'])
            invoke('translate_text',{'text':'An Agent completes a task.'})
            wait(lambda:invoke('get_job')['status']=='success')
            count=len(requests)
            invoke('translate_text',{'text':'An Agent completes a task.'})
            wait(lambda:invoke('get_job')['status']=='success')
            check('repeat translation uses session cache without HTTP',invoke('get_job')['cacheHit'] and len(requests)==count)
            check('cache hit does not inflate term observations',invoke('get_memory',{'query':'','offset':0})['terms'][0]['count']==1)
            wait(lambda:'来自缓存' in body())
            check('native view displays cache provenance',True)
            config_path.with_suffix('.recovery').write_text('settings recovery required\n')
            blocked=invoke('translate_text',{'text':'An Agent completes a task.'},True)
            check('cached result cannot bypass settings recovery gate','error' in blocked and len(requests)==count)
            config_path.with_suffix('.recovery').unlink()
            invoke('translate_text',{'text':'An Agent completes a task.','bypassCache':True})
            wait(lambda:invoke('get_job')['status']=='success')
            check('explicit refresh bypasses and replaces cache',not invoke('get_job')['cacheHit'] and len(requests)==count+1)
            invoke('clear_translation_cache')
            invoke('translate_text',{'text':'An Agent completes a task.'})
            wait(lambda:invoke('get_job')['status']=='success')
            check('clearing cache requires a fresh provider request',not invoke('get_job')['cacheHit'] and len(requests)==count+2)
            count=len(requests)
            for _ in range(2):
                invoke('translate_text',{'text':'cache-error-fixture'})
                wait(lambda:invoke('get_job')['status']=='error')
            check('failed translations are never cached',len(requests)==count+2)
            count=len(requests)
            for _ in range(2):invoke('test_connection',{'config':current,'apiKey':'fixture-provider-key'})
            check('connection tests always contact provider',len(requests)==count+2)
            invoke('translate_text',{'text':'An Agent completes a task.'})
            wait(lambda:invoke('get_job')['status']=='success')
            invoke('analyze_memory')
            wait(lambda:not invoke('get_analysis_status')['running'])
            memory=invoke('get_memory',{'query':'','offset':0})
            check('real translation and profile persisted',len(memory['versions'])==1 and memory['total']==1)
            count=len(requests)
            check('keyword IPC spans name and standalone sense',invoke('get_memory',{'query':'Agent 自主','mode':'keyword','offset':0})['total']==1)
            check('fuzzy IPC retrieves misspelling',invoke('get_memory',{'query':'Agnt','mode':'fuzzy','offset':0})['total']==1)
            check('invalid search mode is recoverable','error' in invoke('get_memory',{'query':'Agent','mode':'invalid','offset':0},True))
            js("Array.from(document.querySelectorAll('nav button')).find(e=>e.textContent.includes('术语记忆')).click()")
            wait(lambda:js("return !!document.querySelector('#term-search')"))
            js("const e=document.querySelector('#term-search');e.value='Agnt';e.dispatchEvent(new Event('input',{bubbles:true}));const m=document.querySelector('#term-search-mode');m.value='fuzzy';m.dispatchEvent(new Event('change',{bubbles:true}));")
            wait(lambda:js("return document.querySelectorAll('.term-card').length")==1)
            check('native search UI retains feedback and delete',js("return !!document.querySelector('.term-card select') && document.querySelector('.term-card').textContent.includes('删除')"))
            check('search does not call model provider',len(requests)==count)
            (out/'memory-search.png').write_bytes(base64.b64decode(http('GET',prefix+'/screenshot')))
            # Own PRIMARY and send a real global shortcut only inside this isolated Xvfb.
            selection_owner=subprocess.Popen([str(Path(a.xclip).resolve()),'-selection','primary','-in','-quiet'],stdin=subprocess.PIPE,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,env=env)
            selection_owner.stdin.write(b'An Agent completes a task.');selection_owner.stdin.close()
            time.sleep(.2)
            x=ctypes.CDLL('libX11.so.6');x.XOpenDisplay.restype=ctypes.c_void_p
            x.XDefaultRootWindow.argtypes=[ctypes.c_void_p];x.XDefaultRootWindow.restype=ctypes.c_ulong
            x.XKeysymToKeycode.argtypes=[ctypes.c_void_p,ctypes.c_ulong];x.XKeysymToKeycode.restype=ctypes.c_uint
            x.XSync.argtypes=[ctypes.c_void_p,ctypes.c_int];x.XCloseDisplay.argtypes=[ctypes.c_void_p]
            x.XWarpPointer.argtypes=[ctypes.c_void_p,ctypes.c_ulong,ctypes.c_ulong,ctypes.c_int,ctypes.c_int,ctypes.c_uint,ctypes.c_uint,ctypes.c_int,ctypes.c_int]
            x.XDisplayWidth.argtypes=[ctypes.c_void_p,ctypes.c_int];x.XDisplayHeight.argtypes=[ctypes.c_void_p,ctypes.c_int]
            xt=ctypes.CDLL('libXtst.so.6');xt.XTestFakeKeyEvent.argtypes=[ctypes.c_void_p,ctypes.c_uint,ctypes.c_int,ctypes.c_ulong]
            display=x.XOpenDisplay(None);root=x.XDefaultRootWindow(display)
            width=x.XDisplayWidth(display,0);height=x.XDisplayHeight(display,0)
            def shortcut_at(px,py):
                previous=invoke('get_job')['revision']
                x.XWarpPointer(display,0,root,0,0,0,0,px,py);x.XSync(display,0)
                codes=[x.XKeysymToKeycode(display,k) for k in (0xffe3,0xffe1,ord('q'))]
                for code in codes:xt.XTestFakeKeyEvent(display,code,1,0)
                for code in reversed(codes):xt.XTestFakeKeyEvent(display,code,0,0)
                x.XSync(display,0)
                wait(lambda:invoke('get_job')['revision']>previous and invoke('get_job')['status']=='success')
                tree=subprocess.check_output(['xwininfo','-root','-tree'],env=env,text=True)
                (out/f'windows-{px}-{py}.txt').write_text(tree)
                match=re.search(r'"划词翻译[^"\n]*".*?(\d+)x(\d+)[+-]\d+[+-]\d+\s+([+-]\d+)([+-]\d+)',tree)
                assert match,tree
                return tuple(map(int,match.groups()))
            w,h,px,py=shortcut_at(500,300)
            print('PLACEMENT',w,h,px,py,'WARNING',invoke('get_job')['warning'],flush=True)
            check('real X11 shortcut captures PRIMARY before focus',invoke('get_job')['original']=='An Agent completes a task.' and invoke('get_job')['source']=='x11-primary')
            check('real X11 popup opens beside pointer',abs(px-512)<=3 and abs(py-316)<=30 and w==440 and h==600)
            w,h,px,py=shortcut_at(width-10,height-10)
            check('real X11 edge popup remains in work area',px>=8 and py>=8 and px+w<=width-8 and py+h<=height-8 and px<width-10 and py<height-10)
            x.XCloseDisplay(display)
            memory=invoke('get_memory',{'query':'','offset':0})
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
            wait(lambda:'已注册' in js("return document.querySelector('[data-testid=shortcut-settings]').innerText"))
            js("document.querySelector('[data-testid=shortcut-settings]').scrollIntoView({block:'center'})")
            (out/'shortcut-settings.png').write_bytes(base64.b64decode(http('GET',prefix+'/screenshot')))
            # Run a real asynchronous request, then update settings before releasing the local provider.
            invoke('translate_text',{'text':'A slow Agent completes a task.'})
            wait(lambda:'slow' in requests[-1]['messages'][1]['content'])
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
            check('compact popup content scrolls with usable controls',js("const m=document.querySelector('main');m.scrollTop=m.scrollHeight;return m.scrollHeight>m.clientHeight && m.scrollTop>0 && !!document.querySelector('textarea') && !!document.querySelector('[aria-label=\"复制译文\"]') && !!document.querySelector('.term-card')"))
            (out/'native-result-scrolled.png').write_bytes(base64.b64decode(http('GET',prefix+'/screenshot')))
            js("document.querySelector('[aria-label=\"复制译文\"]').click()")
            wait(lambda:js("return !!document.querySelector('[aria-label=\"已复制\"]')"))
            check('compact popup copy writes real X11 clipboard',subprocess.check_output([str(Path(a.xclip).resolve()),'-selection','clipboard','-out'],env=env,text=True)=='智能体完成任务。'*50)
            js("document.querySelector('main').scrollTop=0")
            # Recovery gate uses only a non-secret file; it blocks all three model entry paths.
            config_path.with_suffix('.recovery').write_text('settings recovery required\n')
            count=len(requests)
            for name,args in [('translate_text',{'text':'Agent'}),('test_connection',{'config':external,'apiKey':'fixture-provider-key'}),('analyze_memory',{})]:
                check('recovery gate blocks '+name,'error' in invoke(name,args,True))
            check('recovery gate issued no provider requests',len(requests)==count)
            invoke('save_settings',{'config':external,'apiKey':'fixture-provider-key','removeApiKey':False})
            check('successful explicit recovery clears marker',not config_path.with_suffix('.recovery').exists())
            check('recovery preserves existing terms',invoke('get_memory',{'query':'','offset':0})['total']==1)
            invoke('save_settings',{'config':external,'apiKey':None,'removeApiKey':True})
            count=len(requests)
            check('both search modes work without an API key',not invoke('get_settings')['hasApiKey'] and all(invoke('get_memory',{'query':q,'mode':m,'offset':0})['total']==1 for q,m in [('自主','keyword'),('Agnt','fuzzy')]) and len(requests)==count)
            if a.sync_server:
                count=len(requests)
                usage_before_sync=invoke('get_usage')['cumulative']
                check('sync defaults disabled',not invoke('get_sync')['enabled'])
                args={'url':sync_url,'password':'fixture-native-sync-password-only','deviceName':'Native test'}
                check('native sync check sends no memory', '未上传' in invoke('test_sync_connection',args))
                connected=invoke('connect_sync',{**args,'consent':True,'replaceTarget':False,'importLocal':False})
                check('native enrollment exposes no credentials',connected['enabled']and not connected['background']and 'token' not in connected and 'password' not in connected)
                synced=invoke('sync_now')
                check('native keyring token synchronizes',synced['lastSuccess']>0 and synced['pending']==0)
                check('native sync never calls model',len(requests)==count)
                devices=invoke('sync_devices');check('native device list',len(devices)==1)
                handles=http('GET',prefix+'/window/handles')
                for handle in handles:
                    http('POST',prefix+'/window',{'handle':handle})
                    if 'translation' not in js('return location.search'):break
                invoke('show_settings');wait(lambda:js("return !!document.querySelector('#sync-url')"))
                check('native image icon loaded',js("return !!document.querySelector('.brand img')?.naturalWidth"))
                check('native tray uses supplied image',wait(lambda:(out/'tray-icon.json').exists(),5))
                # A remote preference survives a real filesystem publish failure and notifies a dirty draft.
                js("const e=document.querySelector('#model');e.value='sync-unsaved-draft';e.dispatchEvent(new Event('input',{bubbles:true}));")
                def remote(method,path,data=None,token=None):
                    headers={'Content-Type':'application/json'}
                    if token:headers['Authorization']='Bearer '+token
                    req=urllib.request.Request(sync_url+path,data=None if data is None else json.dumps(data).encode(),method=method,headers=headers)
                    with urllib.request.urlopen(req,timeout=10)as response:return json.load(response)
                helper=remote('POST','/v1/devices/enroll',{'protocol':1,'password':'fixture-native-sync-password-only','deviceName':'Preference fixture'})
                page=remote('GET','/v1/changes?stream=preferences&after=0',token=helper['token'])
                theme_revision=max(r['revision']for r in page['changes']if r['change'].get('field')=='theme')
                context=helper['context']
                remote('POST','/v1/push',{'protocol':1,'workspaceId':context['workspaceId'],'historyId':context['historyId'],'operations':[{'id':'native-preference-update','generation':1,'action':{'type':'preference','field':'theme','baseRevision':theme_revision,'value':'light'}}]},helper['token'])
                config_path.parent.chmod(0o500)
                try:failed=invoke('sync_now',allow_error=True)
                finally:config_path.parent.chmod(0o700)
                check('remote preference publish failure is recoverable','error'in failed and invoke('get_settings')['settings']['theme']!='light')
                invoke('configure_sync',{'enabled':True,'background':False});invoke('sync_now')
                check('durable inbox retries without replaying cloud changes',invoke('get_settings')['settings']['theme']=='light'and invoke('get_settings')['settings']['model']=='external-model')
                wait(lambda:js("return !!document.querySelector('[data-testid=settings-external-update]')"))
                check('remote preference preserves native dirty draft',js("return document.querySelector('#model').value")=='sync-unsaved-draft')
                js("document.querySelector('#sync-url').scrollIntoView({block:'start'})")
                (out/'sync-settings.png').write_bytes(base64.b64decode(http('GET',prefix+'/screenshot')))
                invoke('configure_sync',{'enabled':False,'background':False})
                check('native disconnect preserves translation',not invoke('get_sync')['enabled']and invoke('get_job')['status']=='success')
                invoke('configure_sync',{'enabled':True,'background':False})
                check('global clear requires native confirmation','error' in invoke('clear_sync_memory',{'confirmAllDevices':False},True))
                invoke('clear_sync_memory',{'confirmAllDevices':True});invoke('sync_now')
                check('native global clear converges',invoke('get_memory',{'query':'','offset':0})['total']==0)
                invoke('revoke_sync_device',{'id':devices[0]['id']})
                check('native self revocation stops sync',invoke('get_sync')['status']=='needs-auth')
                check('sync and global terminology clear preserve usage',invoke('get_usage')['cumulative']==usage_before_sync)
                exported=remote('GET','/v1/changes?stream=preferences&after=0',token=helper['token'])
                check('usage never enters sync preference payload',all(x not in json.dumps(exported) for x in ['usage.sqlite3','prompt_tokens','total_tokens','fixture-provider-key']))
            # Numeric accounting through real IPC and SQLite, using no paid API.
            for handle in http('GET',prefix+'/window/handles'):
                http('POST',prefix+'/window',{'handle':handle})
                if 'translation' not in js('return location.search'):break
            current=invoke('get_settings')['settings'];current.update({'rememberTerms':True,'autoAnalyze':False,'personalizeExplanations':False,'outputMode':'auto'})
            invoke('save_settings',{'config':current,'apiKey':'fixture-provider-key','removeApiKey':False})
            check('usage clear requires confirmation','error' in invoke('clear_usage',{'confirm':False},True))
            invoke('clear_usage',{'confirm':True})
            count=len(requests)
            invoke('translate_text',{'text':'usage-retry-fixture'})
            wait(lambda:invoke('get_job')['status']=='success')
            job=invoke('get_job');usage=invoke('get_usage')
            check('usage counts each compatibility HTTP attempt once',len(requests)==count+2 and job['usage']['usage']['attempts']==2 and job['usage']['usage']['total']['tokens']=='135' and usage['cumulative']['total']['tokens']=='135')
            invoke('translate_text',{'text':'usage-retry-fixture'});wait(lambda:invoke('get_job')['status']=='success')
            check('usage cache hit adds zero and no provider calls',len(requests)==count+2 and invoke('get_job')['usage']['source']=='local-cache' and invoke('get_usage')['cumulative']['total']['tokens']=='135')
            invoke('translate_text',{'text':'usage-retry-fixture','bypassCache':True});wait(lambda:invoke('get_job')['status']=='success')
            check('usage forced refresh adds its new attempts',invoke('get_usage')['cumulative']['total']['tokens']=='270')
            for text in ['usage-truncated-fixture','usage-refused-fixture','usage-bad-json-fixture']:
                invoke('translate_text',{'text':text});wait(lambda:invoke('get_job')['status']=='error')
                check('usage survives '+text,invoke('get_job')['usage']['usage']['total']['tokens']=='130')
            invoke('translate_text',{'text':'usage-unknown-fixture'});wait(lambda:invoke('get_job')['status']=='success')
            check('missing usage remains unknown not zero',invoke('get_job')['usage']['usage']['total']['tokens'] is None and invoke('get_usage')['cumulative']['total']['unknown']==1)
            tested=invoke('test_connection',{'config':{**current,'model':'usage-invalid-model'},'apiKey':'fixture-temporary-key'})
            check('failed unsaved connection test preserves usage',tested['error'] is not None and tested['usage']['usage']['total']['tokens']=='130' and invoke('get_settings')['settings']['model']!= 'usage-invalid-model')
            filtered=invoke('get_usage',{'query':{'purpose':'connection-test','limit':20}})
            check('usage purpose filter isolates connection tests',filtered['cumulative']['total']['tokens']=='130' and filtered['groups'][0]['model']=='usage-invalid-model')
            # Prepare memory evidence, then overlap analysis with a held translation.
            invoke('translate_text',{'text':'An Agent usage concurrency fixture.'});wait(lambda:invoke('get_job')['status']=='success')
            invoke('translate_text',{'text':'usage-hold-fixture'});wait(lambda:any(r['messages'][1]['content']=='usage-hold-fixture' for r in requests))
            invoke('analyze_memory');wait(lambda:not invoke('get_analysis_status')['running'])
            check('analysis and translation usage remain independent',invoke('get_analysis_status')['usage']['usage']['total']['tokens']=='130' and invoke('get_usage')['cumulative']['pending']==1)
            invoke('clear_usage',{'confirm':True});usage_delay.set();wait(lambda:invoke('get_job')['status']=='success')
            check('late result stays visible but cannot refill reset totals',invoke('get_job')['usage']['excludedAfterReset'] and invoke('get_job')['usage']['usage']['total']['tokens']=='130' and invoke('get_usage')['cumulative']['attempts']==0)
            invoke('translate_text',{'text':'An Agent usage after reset.'});wait(lambda:invoke('get_job')['status']=='success')
            for _ in range(20):
                invoke('translate_text',{'text':'An Agent automatic usage fixture.','bypassCache':True});wait(lambda:invoke('get_job')['status']=='success')
            current['autoAnalyze']=True
            invoke('save_settings',{'config':current,'apiKey':None,'removeApiKey':False})
            wait(lambda:invoke('get_memory',{'query':'','offset':0})['lastAutoAttempt']>0,75)
            wait(lambda:not invoke('get_analysis_status')['running'])
            check('automatic topic analysis reports its own usage',invoke('get_analysis_status')['usage']['purpose']=='auto-analysis' and invoke('get_analysis_status')['usage']['usage']['total']['tokens']=='130')
            current['autoAnalyze']=False
            invoke('save_settings',{'config':current,'apiKey':None,'removeApiKey':False})
            total_before_cancel=invoke('get_usage')['cumulative']['total']['tokens']
            invoke('show_settings');wait(lambda:js("return !!document.querySelector('.usage-settings')"))
            count=len(requests)
            for theme in ['light','dark']:
                js("document.documentElement.dataset.theme=arguments[0];document.querySelector('.usage-settings').scrollIntoView({block:'start'})",theme)
                (out/('usage-settings-'+theme+'.png')).write_bytes(base64.b64decode(http('GET',prefix+'/screenshot')))
            check('usage settings read does not call provider',len(requests)==count and 'Token 用量' in body())
            def enter_key():
                http('POST',prefix+'/actions',{'actions':[{'type':'key','id':'usage-keyboard','actions':[{'type':'keyDown','value':'\ue007'},{'type':'keyUp','value':'\ue007'}]}]})
            js("document.querySelector('.usage-help summary').focus()")
            enter_key();wait(lambda:js("return document.querySelector('.usage-help').open"))
            check('usage help is local and keyboard reachable',js("return document.querySelector('.usage-help').textContent.includes('不是账户配额') && document.querySelector('.usage-help summary').tabIndex===0"))
            js("const e=document.querySelector('#model');e.value='unsaved-usage-draft';e.dispatchEvent(new Event('input',{bubbles:true}));")
            js("Array.from(document.querySelectorAll('.usage-settings button')).find(e=>e.textContent==='清空用量统计').click()")
            wait(lambda:js("return !!document.querySelector('.usage-confirm')"))
            js("Array.from(document.querySelectorAll('.usage-confirm button')).find(e=>e.textContent==='取消清空').click()")
            check('usage cancellation preserves draft and current totals',invoke('get_usage')['cumulative']['total']['tokens']==total_before_cancel and js("return document.querySelector('#model').value")=='unsaved-usage-draft')
            before_keyboard=invoke('get_memory',{'query':'','offset':0})
            js("Array.from(document.querySelectorAll('.usage-settings button')).find(e=>e.textContent==='清空用量统计').focus()")
            enter_key();wait(lambda:js("return !!document.querySelector('.usage-confirm')"))
            js("Array.from(document.querySelectorAll('.usage-confirm button')).find(e=>e.textContent==='确认清空用量').focus()")
            enter_key();wait(lambda:invoke('get_usage')['cumulative']['attempts']==0)
            check('keyboard confirmed usage clear preserves model draft and memory',js("return document.querySelector('#model').value")=='unsaved-usage-draft' and invoke('get_memory',{'query':'','offset':0})==before_keyboard)
            invoke('translate_text',{'text':'An Agent keyboard reset fixture.'});wait(lambda:invoke('get_job')['status']=='success')
            # A narrow real popup renders observed usage without horizontal overflow.
            invoke('show_popup')
            for handle in http('GET',prefix+'/window/handles'):
                http('POST',prefix+'/window',{'handle':handle})
                if 'translation' in js('return location.search'):break
            http('POST',prefix+'/window/rect',{'width':360,'height':600})
            wait(lambda:'本次 Token' in body())
            check('narrow popup usage keeps copy controls reachable',js("return document.documentElement.scrollWidth<=innerWidth && !!document.querySelector('[aria-label=复制译文]')"))
            (out/'usage-popup-narrow.png').write_bytes(base64.b64decode(http('GET',prefix+'/screenshot')))
            js("document.documentElement.dataset.theme='dark'")
            (out/'usage-popup-narrow-dark.png').write_bytes(base64.b64decode(http('GET',prefix+'/screenshot')))
            usage_path=base/'data/io.github.ai-selection-assistant/usage.sqlite3'
            raw=usage_path.read_bytes()
            check('usage store excludes content and secret markers',all(marker not in raw for marker in [b'fixture-provider-key',b'fixture-temporary-key',b'private-source-marker',b'An Agent',b'choices',b'messages']))
            # Fixed-command warm activation, then a cold CLI session. Synthetic text/local provider only.
            clipboard_owner=subprocess.Popen([str(Path(a.xclip).resolve()),'-selection','clipboard','-in','-quiet'],stdin=subprocess.PIPE,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,env=env)
            clipboard_owner.stdin.write(b'An Agent from clipboard.');clipboard_owner.stdin.close()
            time.sleep(.15)
            count=len(requests);old_revision=invoke('get_job')['revision']
            subprocess.run([str(Path(a.binary).resolve()),'--translate-clipboard'],env=env,check=True,timeout=10,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
            wait(lambda:invoke('get_job')['revision']>old_revision and invoke('get_job')['status']=='success')
            check('fixed command forwards exactly one clipboard job',invoke('get_job')['original']=='An Agent from clipboard.' and len(requests)==count+1)
            # Empty clipboard errors must not call the provider.
            clipboard_owner=subprocess.Popen([str(Path(a.xclip).resolve()),'-selection','clipboard','-in','-quiet'],stdin=subprocess.PIPE,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,env=env)
            clipboard_owner.stdin.close();time.sleep(.15)
            count=len(requests);old_revision=invoke('get_job')['revision']
            subprocess.run([str(Path(a.binary).resolve()),'--translate-clipboard'],env=env,check=True,timeout=10,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
            wait(lambda:invoke('get_job')['revision']>old_revision and invoke('get_job')['status']=='error')
            check('clipboard capture error sends no provider request',len(requests)==count)
            clipboard_owner=subprocess.Popen([str(Path(a.xclip).resolve()),'-selection','clipboard','-in','-quiet'],stdin=subprocess.PIPE,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,env=env)
            clipboard_owner.stdin.write(b'An Agent from clipboard.');clipboard_owner.stdin.close();time.sleep(.15)
            usage_before_restart=invoke('get_usage')['cumulative']
            http('DELETE','/session/'+session);session=None
            time.sleep(.3)
            # Hold the default key in this isolated display before startup: plugin init
            # succeeds but registration must fail, without disabling clipboard commands.
            x.XSetErrorHandler.argtypes=[ctypes.c_void_p];x.XSetErrorHandler.restype=ctypes.c_void_p
            grab_errors=[]
            error_callback=ctypes.CFUNCTYPE(ctypes.c_int,ctypes.c_void_p,ctypes.c_void_p)(lambda *_:grab_errors.append(True) or 0)
            old_error_handler=x.XSetErrorHandler(ctypes.cast(error_callback,ctypes.c_void_p))
            reserved_display=x.XOpenDisplay(None)
            reserved_root=x.XDefaultRootWindow(reserved_display);reserved_key=x.XKeysymToKeycode(reserved_display,ord('Q'))
            x.XGrabKey(reserved_display,reserved_key,5,reserved_root,1,1,1);x.XSync(reserved_display,0)
            check('isolated fixture reserves startup key',not grab_errors)
            session=http('POST' ,'/session',{'capabilities':{'alwaysMatch':{'browserName':'wry','tauri:options':{'application':str(Path(a.binary).resolve()),'args':['--translate-clipboard']}}}})['sessionId']
            prefix='/session/'+session
            wait(lambda:invoke('get_job')['status']=='success')
            for handle in http('GET',prefix+'/window/handles'):
                http('POST',prefix+'/window',{'handle':handle})
                if 'translation' in js('return location.search'):break
            check('plugin initialization does not imply shortcut ready',invoke('get_shortcut_status')['capable'] and invoke('get_shortcut_status')['status']=='unavailable')
            check('cold clipboard command captures before result',invoke('get_job')['original']=='An Agent from clipboard.' and invoke('get_job')['source']=='clipboard')
            check('restarting process starts with an empty translation cache',not invoke('get_job')['cacheHit'])
            after_restart=invoke('get_usage')['cumulative']
            check('usage survives actual process restart',after_restart['attempts']==usage_before_restart['attempts']+1 and int(after_restart['total']['tokens'])==int(usage_before_restart['total']['tokens'])+130)
            check('cold clipboard launch keeps main hidden',not invoke('plugin:window|is_visible',{'label':'main'}))
            check('cold clipboard launch opens result popup',invoke('plugin:window|is_visible',{'label':'translation'}))
            # WebKit's screenshot endpoint waits forever for a hidden main webview.
            # Capture this isolated Xvfb root to include the visible result popup.
            from PIL import ImageGrab
            ImageGrab.grab().save(out/'native-result.png')
            count=len(requests)
            config_path.with_suffix('.recovery').write_text('settings recovery required\n')
            subprocess.run([str(Path(a.binary).resolve()),'--translate-clipboard'],env=env,check=True,timeout=10,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
            wait(lambda:invoke('get_job')['status']=='error')
            check('command rejection surfaces recovery error without model call',invoke('get_job')['error']['code']=='settings_recovery' and len(requests)==count and invoke('plugin:window|is_visible',{'label':'translation'}))
            (out/'result.json').write_text(json.dumps({'passed':len(results),'checks':results,'providerRequests':len(requests)},ensure_ascii=False,indent=2))
    finally:
        delay.set()
        usage_delay.set()
        if session:
            try:http('DELETE','/session/'+session)
            except Exception:pass
        if driver and driver.poll() is None:
            os.killpg(driver.pid,signal.SIGTERM)
            driver.wait(timeout=5)
        if reserved_display:
            x.XCloseDisplay(reserved_display);x.XSetErrorHandler(old_error_handler)
        if clipboard_owner and clipboard_owner.poll() is None:
            clipboard_owner.terminate();clipboard_owner.wait(timeout=5)
        if selection_owner and selection_owner.poll() is None:
            selection_owner.terminate();selection_owner.wait(timeout=5)
        if 'wm' in locals() and wm.poll() is None:
            wm.terminate();wm.wait(timeout=5)
        if tray_process and tray_process.poll() is None:
            tray_process.terminate();tray_process.wait(timeout=5)
        if sync_process and sync_process.poll() is None:
            sync_process.terminate();sync_process.wait(timeout=5)
        server.shutdown()

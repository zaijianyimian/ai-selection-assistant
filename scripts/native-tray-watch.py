#!/usr/bin/env python3
"""Isolated StatusNotifier host for verifying the actual native tray icon bytes."""
import hashlib,json,sys
from pathlib import Path
import gi
from gi.repository import Gio,GLib
from PIL import Image
output=Path(sys.argv[1]);expected=Path(sys.argv[2]);items=[]
xml='''<node><interface name="org.kde.StatusNotifierWatcher"><method name="RegisterStatusNotifierItem"><arg type="s" direction="in"/></method><method name="RegisterStatusNotifierHost"><arg type="s" direction="in"/></method><property name="ProtocolVersion" type="i" access="read"/><property name="IsStatusNotifierHostRegistered" type="b" access="read"/><property name="RegisteredStatusNotifierItems" type="as" access="read"/><signal name="StatusNotifierItemRegistered"><arg type="s"/></signal><signal name="StatusNotifierHostRegistered"/></interface></node>'''
interface=Gio.DBusNodeInfo.new_for_xml(xml).interfaces[0]
def method(bus,sender,path,iface,name,args,invocation):
 if name=='RegisterStatusNotifierItem':
  value=args.unpack()[0];service=sender if value.startswith('/')else value;object_path=value if value.startswith('/')else '/StatusNotifierItem';items.append((service,object_path));print('Registered',service,object_path,flush=True)
  bus.emit_signal(None,'/StatusNotifierWatcher','org.kde.StatusNotifierWatcher','StatusNotifierItemRegistered',GLib.Variant('(s)',(service+object_path,)))
 invocation.return_value(GLib.Variant('()',()))
def property_value(bus,sender,path,iface,name):return {'ProtocolVersion':GLib.Variant('i',0),'IsStatusNotifierHostRegistered':GLib.Variant('b',True),'RegisteredStatusNotifierItems':GLib.Variant('as',[s+p for s,p in items])}[name]
def acquired(bus,name):bus.register_object('/StatusNotifierWatcher',interface,method,property_value,None)
bus=Gio.bus_get_sync(Gio.BusType.SESSION,None);Gio.bus_own_name_on_connection(bus,'org.kde.StatusNotifierWatcher',Gio.BusNameOwnerFlags.NONE,None,None)
acquired(bus,None)
def poll():
 for service,path in items:
  try:
   props=bus.call_sync(service,path,'org.freedesktop.DBus.Properties','GetAll',GLib.Variant('(s)',('org.kde.StatusNotifierItem',)),GLib.VariantType.new('(a{sv})'),Gio.DBusCallFlags.NONE,2000,None).unpack()[0]
   if 'rustdesk' in props.get('Id','').lower():continue
   icon=props.get('IconName','');theme=props.get('IconThemePath','');output.with_suffix('.probe.json').write_text(json.dumps({'properties':list(props),'icon':icon,'theme':theme}))
   candidates=[Path(icon),Path(theme)/(icon+'.png')]
   if theme and not Path(icon).is_absolute():candidates+=list(Path(theme).rglob(icon+'.png'))
   for candidate in candidates:
    if candidate.is_file():
     actual=Image.open(candidate).convert('RGBA');original=Image.open(expected).convert('RGBA');assert actual.size==original.size and actual.tobytes()==original.tobytes(),'native tray differs from application icon'
     output.write_text(json.dumps({'passed':True,'width':actual.width,'height':actual.height,'rgbaSha256':hashlib.sha256(actual.tobytes()).hexdigest()}));return False
  except Exception as e:print(type(e).__name__,str(e),flush=True)
 return True
GLib.timeout_add(200,poll);GLib.MainLoop().run()

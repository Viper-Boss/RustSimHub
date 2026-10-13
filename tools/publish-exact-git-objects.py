import base64,datetime,json,re,subprocess
repo='repos/Viper-Boss/RustSimHub'
def run(argv,input=None):
 r=subprocess.run(argv,input=input,stdout=subprocess.PIPE,stderr=subprocess.PIPE,check=True);return r.stdout

def git(*a):return run(['git',*a])
def api(path,payload=None,method=None):
 args=['gh','api',repo+'/'+path]
 if payload is not None:args+=['--method',method or 'POST','--input','-']
 return json.loads(run(args,json.dumps(payload).encode() if payload is not None else None))
old=api('git/ref/heads/main')['object']['sha'];head=git('rev-parse','HEAD').decode().strip()
run(['git','merge-base','--is-ancestor',old,head])
commits=git('rev-list','--reverse',old+'..'+head).decode().splitlines()
assert len(commits)<=8
for sha in commits:
 raw=git('cat-file','commit',sha).decode();headers,message=raw.split('\n\n',1);fields={};parents=[]
 for h in headers.splitlines():
  k,v=h.split(' ',1)
  if k=='parent':parents.append(v)
  else:assert k in ['tree','author','committer'];fields[k]=v
 assert len(parents)==1
 entries=[]
 names=git('diff-tree','--no-commit-id','--name-only','-r','-z',sha).split(b'\0')
 for n in names:
  if not n:continue
  name=n.decode();assert not name.startswith(('.private/','.local/','target/'))
  row=git('ls-tree',sha,'--',name).decode().strip();assert row
  mode,kind,blob=row.split('\t')[0].split();assert kind=='blob' and mode=='100644'
  b=git('cat-file','blob',blob)
  remote=api('git/blobs',{'content':base64.b64encode(b).decode(),'encoding':'base64'})
  assert remote['sha']==blob
  entries.append({'path':name,'mode':mode,'type':'blob','sha':blob})
 tree=api('git/trees',{'base_tree':git('rev-parse',parents[0]+'^{tree}').decode().strip(),'tree':entries})
 assert tree['sha']==fields['tree']
 def identity(v):
  m=re.fullmatch(r'(.*) <([^>]+)> (\d+) ([+-]\d{4})',v);assert m
  name,email,epoch,offset=m.groups();minutes=int(offset[1:3])*60+int(offset[3:]);minutes*=1 if offset[0]=='+' else -1
  date=datetime.datetime.fromtimestamp(int(epoch),datetime.timezone(datetime.timedelta(minutes=minutes))).isoformat()
  return {'name':name,'email':email,'date':date}
 remote=api('git/commits',{'tree':fields['tree'],'parents':parents,'message':message,'author':identity(fields['author']),'committer':identity(fields['committer'])})
 assert remote['sha']==sha,'Server changed commit identity: do not update branch'
 print('Verified exact commit object',sha[:12])
assert api('git/ref/heads/main')['object']['sha']==old,'Concurrent remote update'
if commits:
 new=api('git/refs/heads/main',{'sha':head,'force':False},'PATCH');assert new['object']['sha']==head
 git('update-ref','refs/remotes/origin/main',head,old)
print('GitHub main matches local HEAD without rewriting history',head[:12])

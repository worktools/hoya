"""Real-process protocol tests; no network services other than the spawned Hoya."""
import concurrent.futures, hashlib, json, os, pathlib, socket, subprocess, time, urllib.request, urllib.error
ROOT = pathlib.Path(__file__).resolve().parents[1]
with socket.socket() as sock:
    sock.bind(('127.0.0.1', 0)); port = sock.getsockname()[1]
base = f'http://127.0.0.1:{port}'
token = 'protocol-test-only'
process = subprocess.Popen([str(ROOT/'target/debug/hoya')], env={**os.environ, 'PORT':str(port), 'HOYA_BIND':'127.0.0.1', 'HOYA_AUTH_TOKEN':token, 'HOYA_MODE':'engine'}, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
def http(path, data=None, auth=token):
    request=urllib.request.Request(base+path, data=None if data is None else json.dumps(data).encode(), headers={'Content-Type':'application/json', 'Authorization':'Bearer '+auth})
    try:
        with urllib.request.urlopen(request,timeout=15) as response:return response.status,json.load(response)
    except urllib.error.HTTPError as error:
        body=error.read()
        try:return error.code,json.loads(body)
        except ValueError:return error.code,body.decode()
def request(source, **extra):
    return dict(protocolVersion='1',runId='http-test',runtime='javascript',code=source,artifactSha256=hashlib.sha256(source.encode()).hexdigest(),input={'value':'`${x}`'},**extra)
try:
    for attempt in range(200):
        try:
            if http('/health')[0]==200:break
        except OSError:time.sleep(.05)
    else:raise AssertionError('Hoya did not start')
    assert http('/v1/capabilities')[1]['wasmAbi']=='hoya-json-v1'
    good=request('async function main(input, ctx) { ctx.log("info", "ok"); return input; }')
    assert http('/v1/executions',good,auth='wrong')[0]==401
    assert http('/execute/js',good)[0]==404
    status,result=http('/v1/executions',good);assert status==200 and result['status']=='succeeded',result
    assert result['result']==good['input'] and result['runId']==good['runId'] and result['artifactSha256']==good['artifactSha256']
    bad=dict(good,artifactSha256='0'*64);assert http('/v1/executions',bad)[1]['error']['code']=='ARTIFACT_HASH_MISMATCH'
    assert http('/v1/executions',dict(good,capabilities={'network':['example.com']}))[1]['error']['code']=='UNSUPPORTED_CAPABILITY'
    limits=dict(timeoutMs=150,memoryMb=8,maxLogBytes=1024,maxResultBytes=1024)
    loop=request('function main() { while(true) {} }',limits=limits)
    status,result=http('/v1/executions',loop);assert result['status']=='timed_out',result
    assert http('/v1/executions',good)[1]['status']=='succeeded'
    flooding=request('function main(x,ctx) { for(let i=0;i<10000;i++)ctx.log("info","x"); return 1; }',limits=dict(limits,timeoutMs=3000))
    assert http('/v1/executions',flooding)[1]['error']['code']=='LOG_LIMIT'
    with concurrent.futures.ThreadPoolExecutor(max_workers=12) as pool:
        results=list(pool.map(lambda _:http('/v1/executions',request('function main(){while(true){}}',limits=dict(limits,timeoutMs=1000))),range(12)))
    assert any(status==429 for status,_ in results),results
    assert http('/v1/executions',good)[1]['status']=='succeeded'
    print('HTTP protocol: auth, legacy exclusion, correlation, hash, capabilities, timeout recovery, log budget and overload passed')
finally:
    process.terminate()
    try:process.wait(timeout=5)
    except subprocess.TimeoutExpired:process.kill();process.wait()

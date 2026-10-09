ObjC.import('Foundation');
let cachedApp;
function operate(argv) {
    const app = cachedApp || (cachedApp = Application('/Applications/iTerm.app'));
    if (argv[0] === 'ping') return 'ok';
    if (!app.running()) throw Error('iTerm2 is not running');
    function variable(s, name) {
        try { return s.variable({named: name}) || ''; } catch (_) { return ''; }
    }
    function resolve(id) {
        // Fast path uses the cached containers only to resolve a UUID, never to
        // select an index. A moved tab falls back to a fresh bulk UUID lookup.
        if (argv.length >= 7) {
            try {
                const w = app.windows.byId(Number(argv[4]));
                const t = w.tabs[Number(argv[5])];
                const s = t.sessions.byId(id);
                if (s.id() === id) return {w:w, t:t, s:s};
            } catch (_) {}
        }
        const ids = app.windows.tabs.sessions.id();
        for (let wi=0; wi<ids.length; wi++)
            for (let ti=0; ti<ids[wi].length; ti++)
                if (ids[wi][ti].indexOf(id) !== -1) {
                    const w=app.windows[wi], t=w.tabs[ti];
                    return {w:w,t:t,s:t.sessions.byId(id)};
                }
        throw Error('Pane no longer exists');
    }
    if (argv[0] === 'observe') {
        const panes=JSON.parse(argv[1]);
        const texts=panes.map(function(p) {
            try {
                let s;
                try {
                    s=app.windows.byId(p.window_id).tabs[p.tab_index].sessions.byId(p.id);
                    // byId is the UUID selector, not an index. Testing TTY also
                    // forces resolution; moved tabs use the fresh UUID fallback.
                    if (s.tty() !== p.tty) return null;
                } catch (_) {
                    s=resolve(p.id).s;
                    if (s.tty() !== p.tty) return null;
                }
                if (Number(variable(s,'pid')) !== p.root_pid) return null;
                // Only the currently visible screen, bounded to its final 24
                // rows; no archived/full conversation reads for status polling.
                return Array.from(s.contents().split('\n').slice(-24).join('\n')).slice(-8192).join('');
            } catch (_) { return null; }
        });
        let focused=null;
        try { if (app.frontmost()) focused=app.currentWindow().currentSession().id(); } catch (_) {}
        return JSON.stringify({texts:texts,focused:focused});
    }
    if (argv[0] === 'jump' || argv[0] === 'content') {
        const p=resolve(argv[1]), s=p.s;
        if (s.id() !== argv[1] || s.tty() !== argv[2] || String(variable(s,'pid')) !== argv[3])
            throw Error('Pane linkage changed; refresh before Jump');
        if (argv[0] === 'content') {
            let text=s.contents().slice(-131074);
            const first=text.charCodeAt(0);
            if (first >= 0xDC00 && first <= 0xDFFF) text=text.slice(1);
            return Array.from(text).slice(-65536).join('');
        }
        p.w.select(); p.t.select(); s.select(); app.activate();
        // The front-window state can lag successful selection briefly.
        // Confirm the UUID within a short bound; never substitute another pane.
        const deadline=Date.now()+250;
        while (app.currentWindow().currentSession().id() !== argv[1]) {
            if (Date.now() >= deadline) throw Error('Jump could not be confirmed');
            $.NSThread.sleepForTimeInterval(0.01);
        }
        return argv[1];
    }
    const ids=app.windows.tabs.sessions.id(), ttys=app.windows.tabs.sessions.tty();
    const names=app.windows.tabs.sessions.name(), titles=app.windows.tabs.title();
    const wins=app.windows.id(), winNames=app.windows.name();
    const found=[];
    for (let wi=0; wi<ids.length; wi++) {
        for (let ti=0; ti<ids[wi].length; ti++) {
            for (let pi=0; pi<ids[wi][ti].length; pi++) {
                const s=app.windows[wi].tabs[ti].sessions.byId(ids[wi][ti][pi]);
                found.push({id:ids[wi][ti][pi], tty:ttys[wi][ti][pi],
                    root_pid:Number(variable(s,'pid')),
                    window_id:wins[wi], window:winNames[wi], tab:titles[wi][ti],
                    tab_index:ti, pane_index:pi, name:names[wi][ti][pi]});
            }
        }
    }
    return JSON.stringify(found);
}

function run(argv) {
    if (argv[0] !== 'server') return operate(argv);
    const input=$.NSFileHandle.fileHandleWithStandardInput;
    const output=$.NSFileHandle.fileHandleWithStandardOutput;
    let pending='';
    while (true) {
        const data=input.availableData;
        if (Number(data.length) === 0) break;
        pending += ObjC.unwrap($.NSString.alloc.initWithDataEncoding(data,$.NSUTF8StringEncoding));
        let end;
        while ((end=pending.indexOf('\n')) !== -1) {
            const line=pending.slice(0,end);pending=pending.slice(end+1);
            let response;
            try { response={ok:true,value:operate(JSON.parse(line))}; }
            catch (e) { cachedApp=null; response={ok:false,error:String(e)}; }
            output.writeData($.NSString.alloc.initWithUTF8String(JSON.stringify(response)+'\n').dataUsingEncoding($.NSUTF8StringEncoding));
        }
    }
    return '';
}

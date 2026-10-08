function run(argv) {
    const app = Application('/Applications/iTerm.app');
    if (!app.running()) throw Error('iTerm2 is not running');
    function variable(s, name) {
        try { return s.variable({named: name}) || ''; } catch (_) { return ''; }
    }
    const found = [];
    for (const w of app.windows()) {
        const tabs = w.tabs();
        for (let ti = 0; ti < tabs.length; ti++) {
            const t = tabs[ti];
            const sessions = t.sessions();
            for (let pi = 0; pi < sessions.length; pi++) {
                const s = sessions[pi];
                const p = {id:s.id(), tty:s.tty(), root_pid:Number(variable(s,'pid')),
                    job_pid:Number(variable(s,'jobPid')), window_id:w.id(), window:w.name(),
                    tab:t.title(), tab_index:ti, pane_index:pi, name:s.name()};
                if (argv[0] === 'list') found.push(p);
                else if (p.id === argv[1]) {
                    if (p.tty !== argv[2] || String(p.root_pid) !== argv[3] || String(p.job_pid) !== argv[4])
                        throw Error('Pane linkage changed; refresh before Jump');
                    // Resolve the current containers; never address stale tab indexes.
                    w.select(); t.select(); s.select(); app.activate();
                    if (app.currentWindow().currentSession().id() !== p.id)
                        throw Error('Jump could not be confirmed');
                    return p.id;
                }
            }
        }
    }
    if (argv[0] === 'list') return JSON.stringify(found);
    throw Error('Pane no longer exists');
}

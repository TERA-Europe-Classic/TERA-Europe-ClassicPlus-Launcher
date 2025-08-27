(function(){
  const { invoke } = window.__TAURI__.tauri;
  const { listen } = window.__TAURI__.event;

  const hostEl = document.getElementById('host');
  const portEl = document.getElementById('port');
  const startBtn = document.getElementById('start');
  const clearBtn = document.getElementById('clear');
  const logEl = document.getElementById('log');

  function appendSys(msg){
    const d = document.createElement('div');
    d.className = 'line sys';
    d.textContent = msg;
    logEl.appendChild(d);
    logEl.scrollTop = logEl.scrollHeight;
  }
  function appendLine(ts, len, hex){
    const d = document.createElement('div');
    d.className = 'line';
    d.textContent = `[${ts}] len=${len} data=${hex}`;
    logEl.appendChild(d);
    logEl.scrollTop = logEl.scrollHeight;
  }

  // Load saved host/port
  hostEl.value = localStorage.getItem('mirror_host') || '127.0.0.1';
  portEl.value = localStorage.getItem('mirror_port') || '7801';

  // Listen to packets emitted by backend
  listen('mirror_packet', (event) => {
    const { ts, len, hex } = event.payload || {};
    appendLine(ts || '', len || 0, hex || '');
  });

  startBtn.addEventListener('click', async () => {
    const host = (hostEl.value || '127.0.0.1').trim();
    const port = parseInt((portEl.value || '7801').trim(), 10) || 7801;
    localStorage.setItem('mirror_host', host);
    localStorage.setItem('mirror_port', String(port));
    try {
      await invoke('start_mirror_client', { host, port });
      appendSys(`Started mirror on ${host}:${port}`);
    } catch (e) {
      appendSys(`Error: ${e}`);
    }
  });

  clearBtn.addEventListener('click', () => {
    logEl.innerHTML = '';
  });

  appendSys('Ready. Enter host/port and click Start.');
})();

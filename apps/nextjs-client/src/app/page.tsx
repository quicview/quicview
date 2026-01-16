'use client';

import { useState, useEffect, useCallback, useRef } from 'react';

interface CtrlStatus {
  connected: boolean;
  reconnects: number;
  attempts: number;
  last_error?: string;
  ping_interval_secs?: number;
  backoff_base_ms?: number;
  backoff_max_ms?: number;
}

interface DisplayInfo {
  id: number;
  x: number;
  y: number;
  width: number;
  height: number;
  is_main: boolean;
}

interface ServerEntry {
  name: string;
  url: string;
  token: string;
}

export default function Home() {
  // Connection settings
  const [baseUrl, setBaseUrl] = useState('http://127.0.0.1:8082');
  const [token, setToken] = useState('');
  
  // Stream settings
  const [width, setWidth] = useState(1280);
  const [height, setHeight] = useState(720);
  const [fps, setFps] = useState(15);
  const [quality, setQuality] = useState(70);
  
  // State
  const [status, setStatus] = useState<'unknown' | 'running' | 'stopped' | 'error'>('unknown');
  const [consentAllowed, setConsentAllowed] = useState(false);
  const [ctrlStatus, setCtrlStatus] = useState<CtrlStatus | null>(null);
  const [remoteControl, setRemoteControl] = useState(false);
  const [displays, setDisplays] = useState<DisplayInfo[]>([]);
  const [selectedDisplay, setSelectedDisplay] = useState<number | null>(null);
  
  // Server registry
  const [servers, setServers] = useState<ServerEntry[]>([]);
  const [selectedServer, setSelectedServer] = useState<number | null>(null);
  const [newServerName, setNewServerName] = useState('');
  
  // Clipboard
  const [clipboard, setClipboard] = useState('');
  
  // Stream viewer ref
  const imgRef = useRef<HTMLImageElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);

  // Load settings from localStorage
  useEffect(() => {
    if (typeof window !== 'undefined') {
      const saved = localStorage.getItem('quicview.settings');
      if (saved) {
        try {
          const s = JSON.parse(saved);
          if (s.baseUrl) setBaseUrl(s.baseUrl);
          if (s.token) setToken(s.token);
          if (s.width) setWidth(s.width);
          if (s.height) setHeight(s.height);
          if (s.fps) setFps(s.fps);
          if (s.quality) setQuality(s.quality);
          if (s.servers) setServers(s.servers);
        } catch {}
      }
    }
  }, []);

  // Save settings to localStorage
  useEffect(() => {
    if (typeof window !== 'undefined') {
      localStorage.setItem('quicview.settings', JSON.stringify({
        baseUrl, token, width, height, fps, quality, servers
      }));
    }
  }, [baseUrl, token, width, height, fps, quality, servers]);

  // Fetch status
  const fetchStatus = useCallback(async () => {
    try {
      const headers: HeadersInit = {};
      if (token) headers['Authorization'] = `Bearer ${token}`;
      const res = await fetch(`${baseUrl}/status`, { headers });
      if (res.ok) {
        const data = await res.json();
        setStatus(data.running ? 'running' : 'stopped');
        setConsentAllowed(data.consent_allowed ?? false);
        if (data.ctrl) setCtrlStatus(data.ctrl);
      } else {
        setStatus('error');
      }
    } catch {
      setStatus('error');
    }
  }, [baseUrl, token]);

  // Fetch displays
  const fetchDisplays = useCallback(async () => {
    try {
      const headers: HeadersInit = {};
      if (token) headers['Authorization'] = `Bearer ${token}`;
      const res = await fetch(`${baseUrl}/displays`, { headers });
      if (res.ok) {
        const data = await res.json();
        setDisplays(data.displays || []);
        if (data.selected !== undefined) setSelectedDisplay(data.selected);
      }
    } catch {}
  }, [baseUrl, token]);

  // Initial fetch
  useEffect(() => {
    fetchStatus();
    fetchDisplays();
    const interval = setInterval(fetchStatus, 3000);
    return () => clearInterval(interval);
  }, [fetchStatus, fetchDisplays]);

  // Actions
  const postAction = async (endpoint: string, body?: object) => {
    try {
      const headers: HeadersInit = { 'Content-Type': 'application/json' };
      if (token) headers['Authorization'] = `Bearer ${token}`;
      await fetch(`${baseUrl}${endpoint}`, {
        method: 'POST',
        headers,
        body: body ? JSON.stringify(body) : undefined
      });
      await fetchStatus();
    } catch {}
  };

  const handleStart = () => postAction('/start');
  const handleStop = () => postAction('/stop');
  const handleAllow = () => postAction('/consent/allow');
  const handleDeny = () => postAction('/consent/deny');

  const handleSelectDisplay = async (id: number) => {
    setSelectedDisplay(id);
    await postAction('/displays/select', { id });
  };

  const handleReadClipboard = async () => {
    try {
      const headers: HeadersInit = {};
      if (token) headers['Authorization'] = `Bearer ${token}`;
      const res = await fetch(`${baseUrl}/clipboard`, { headers });
      if (res.ok) {
        const data = await res.json();
        setClipboard(data.text || '');
      }
    } catch {}
  };

  const handleWriteClipboard = async () => {
    await postAction('/clipboard', { text: clipboard });
  };

  // Presets
  const applyPreset = (preset: 'low' | 'medium' | 'high' | 'ultra') => {
    switch (preset) {
      case 'low': setWidth(640); setHeight(360); setFps(10); setQuality(60); break;
      case 'medium': setWidth(1280); setHeight(720); setFps(15); setQuality(70); break;
      case 'high': setWidth(1920); setHeight(1080); setFps(30); setQuality(80); break;
      case 'ultra': setWidth(2560); setHeight(1440); setFps(60); setQuality(90); break;
    }
  };

  // Server registry
  const addServer = () => {
    const name = newServerName || baseUrl;
    setServers([...servers, { name, url: baseUrl, token }]);
    setNewServerName('');
  };

  const loadServer = (idx: number) => {
    const s = servers[idx];
    if (s) {
      setBaseUrl(s.url);
      setToken(s.token);
      setSelectedServer(idx);
    }
  };

  const removeServer = (idx: number) => {
    setServers(servers.filter((_, i) => i !== idx));
    if (selectedServer === idx) setSelectedServer(null);
  };

  // Stream URL
  const streamUrl = `${baseUrl}/stream.mjpeg?w=${width}&h=${height}&fps=${fps}&q=${quality}${token ? `&token=${encodeURIComponent(token)}` : ''}`;

  // Mouse/keyboard input handlers
  const sendMouse = async (payload: object) => {
    if (!remoteControl) return;
    try {
      const headers: HeadersInit = { 'Content-Type': 'application/json' };
      if (token) headers['Authorization'] = `Bearer ${token}`;
      await fetch(`${baseUrl}/input/mouse`, { method: 'POST', headers, body: JSON.stringify(payload) });
    } catch {}
  };

  const sendKey = async (payload: object) => {
    if (!remoteControl) return;
    try {
      const headers: HeadersInit = { 'Content-Type': 'application/json' };
      if (token) headers['Authorization'] = `Bearer ${token}`;
      await fetch(`${baseUrl}/input/key`, { method: 'POST', headers, body: JSON.stringify(payload) });
    } catch {}
  };

  const mapCoords = (clientX: number, clientY: number): [number, number] => {
    if (!imgRef.current) return [0, 0];
    const rect = imgRef.current.getBoundingClientRect();
    const rx = clientX - rect.left;
    const ry = clientY - rect.top;
    const x = (rx / rect.width) * width;
    const y = (ry / rect.height) * height;
    return [x, y];
  };

  return (
    <div className="min-h-screen bg-gradient-to-br from-slate-900 via-slate-800 to-slate-900 text-white">
      {/* Header */}
      <header className="border-b border-slate-700 bg-slate-900/50 backdrop-blur-sm sticky top-0 z-50">
        <div className="max-w-7xl mx-auto px-4 py-3 flex items-center justify-between">
          <div className="flex items-center gap-3">
            <div className="w-8 h-8 bg-gradient-to-br from-cyan-400 to-blue-600 rounded-lg flex items-center justify-center font-bold text-sm">
              QV
            </div>
            <h1 className="text-xl font-semibold">QuicView</h1>
          </div>
          <div className="flex items-center gap-4">
            <div className={`flex items-center gap-2 px-3 py-1.5 rounded-full text-sm font-medium ${
              status === 'running' ? 'bg-green-500/20 text-green-400' :
              status === 'stopped' ? 'bg-yellow-500/20 text-yellow-400' :
              status === 'error' ? 'bg-red-500/20 text-red-400' :
              'bg-slate-500/20 text-slate-400'
            }`}>
              <div className={`w-2 h-2 rounded-full ${
                status === 'running' ? 'bg-green-400 animate-pulse' :
                status === 'stopped' ? 'bg-yellow-400' :
                status === 'error' ? 'bg-red-400' :
                'bg-slate-400'
              }`} />
              {status.charAt(0).toUpperCase() + status.slice(1)}
            </div>
            {ctrlStatus && (
              <div className={`flex items-center gap-2 px-3 py-1.5 rounded-full text-sm font-medium ${
                ctrlStatus.connected ? 'bg-cyan-500/20 text-cyan-400' : 'bg-slate-500/20 text-slate-400'
              }`}>
                <div className={`w-2 h-2 rounded-full ${ctrlStatus.connected ? 'bg-cyan-400' : 'bg-slate-400'}`} />
                QUIC {ctrlStatus.connected ? 'Connected' : 'Disconnected'}
              </div>
            )}
          </div>
        </div>
      </header>

      <div className="max-w-7xl mx-auto px-4 py-6">
        <div className="grid grid-cols-1 lg:grid-cols-4 gap-6">
          {/* Left sidebar - Controls */}
          <div className="lg:col-span-1 space-y-4">
            {/* Connection */}
            <div className="bg-slate-800/50 rounded-xl p-4 border border-slate-700">
              <h2 className="text-sm font-semibold text-slate-300 uppercase tracking-wide mb-3">Connection</h2>
              <div className="space-y-3">
                <div>
                  <label className="text-xs text-slate-400 block mb-1">Server URL</label>
                  <input
                    type="text"
                    value={baseUrl}
                    onChange={(e) => setBaseUrl(e.target.value)}
                    className="w-full bg-slate-900 border border-slate-600 rounded-lg px-3 py-2 text-sm focus:ring-2 focus:ring-cyan-500 focus:border-transparent outline-none"
                    placeholder="http://127.0.0.1:8082"
                  />
                </div>
                <div>
                  <label className="text-xs text-slate-400 block mb-1">Auth Token</label>
                  <input
                    type="password"
                    value={token}
                    onChange={(e) => setToken(e.target.value)}
                    className="w-full bg-slate-900 border border-slate-600 rounded-lg px-3 py-2 text-sm focus:ring-2 focus:ring-cyan-500 focus:border-transparent outline-none"
                    placeholder="Optional bearer token"
                  />
                </div>
                <button
                  onClick={fetchStatus}
                  className="w-full bg-slate-700 hover:bg-slate-600 text-white rounded-lg px-4 py-2 text-sm font-medium transition-colors"
                >
                  Refresh Status
                </button>
              </div>
            </div>

            {/* Server Registry */}
            <div className="bg-slate-800/50 rounded-xl p-4 border border-slate-700">
              <h2 className="text-sm font-semibold text-slate-300 uppercase tracking-wide mb-3">Saved Servers</h2>
              <div className="space-y-2">
                {servers.map((s, i) => (
                  <div key={i} className={`flex items-center gap-2 p-2 rounded-lg cursor-pointer transition-colors ${
                    selectedServer === i ? 'bg-cyan-500/20 border border-cyan-500/50' : 'bg-slate-900/50 hover:bg-slate-700/50'
                  }`}>
                    <button onClick={() => loadServer(i)} className="flex-1 text-left text-sm truncate">{s.name}</button>
                    <button onClick={() => removeServer(i)} className="text-red-400 hover:text-red-300 text-xs">✕</button>
                  </div>
                ))}
                <div className="flex gap-2 mt-2">
                  <input
                    type="text"
                    value={newServerName}
                    onChange={(e) => setNewServerName(e.target.value)}
                    className="flex-1 bg-slate-900 border border-slate-600 rounded-lg px-2 py-1.5 text-xs focus:ring-2 focus:ring-cyan-500 focus:border-transparent outline-none"
                    placeholder="Server name"
                  />
                  <button onClick={addServer} className="bg-cyan-600 hover:bg-cyan-500 px-3 py-1.5 rounded-lg text-xs font-medium transition-colors">
                    Add
                  </button>
                </div>
              </div>
            </div>

            {/* Display Selection */}
            <div className="bg-slate-800/50 rounded-xl p-4 border border-slate-700">
              <h2 className="text-sm font-semibold text-slate-300 uppercase tracking-wide mb-3">Display</h2>
              <div className="space-y-2">
                {displays.length === 0 ? (
                  <p className="text-xs text-slate-500">No displays detected</p>
                ) : displays.map((d) => (
                  <button
                    key={d.id}
                    onClick={() => handleSelectDisplay(d.id)}
                    className={`w-full flex items-center gap-3 p-2 rounded-lg text-left transition-colors ${
                      selectedDisplay === d.id ? 'bg-cyan-500/20 border border-cyan-500/50' : 'bg-slate-900/50 hover:bg-slate-700/50'
                    }`}
                  >
                    <div className="w-8 h-6 bg-slate-700 rounded border border-slate-600 flex items-center justify-center text-xs">
                      {d.id}
                    </div>
                    <div className="flex-1 text-xs">
                      <div className="font-medium">{d.width}×{d.height}</div>
                      <div className="text-slate-500">Position: {d.x}, {d.y}</div>
                    </div>
                    {d.is_main && <span className="text-xs bg-blue-500/20 text-blue-400 px-2 py-0.5 rounded">Main</span>}
                  </button>
                ))}
                <button onClick={fetchDisplays} className="w-full bg-slate-700 hover:bg-slate-600 text-white rounded-lg px-4 py-2 text-xs font-medium transition-colors">
                  Refresh Displays
                </button>
              </div>
            </div>

            {/* Stream Quality */}
            <div className="bg-slate-800/50 rounded-xl p-4 border border-slate-700">
              <h2 className="text-sm font-semibold text-slate-300 uppercase tracking-wide mb-3">Stream Quality</h2>
              <div className="grid grid-cols-4 gap-2 mb-4">
                {(['low', 'medium', 'high', 'ultra'] as const).map((preset) => (
                  <button
                    key={preset}
                    onClick={() => applyPreset(preset)}
                    className="bg-slate-700 hover:bg-slate-600 text-white rounded-lg px-2 py-1.5 text-xs font-medium capitalize transition-colors"
                  >
                    {preset}
                  </button>
                ))}
              </div>
              <div className="grid grid-cols-2 gap-3">
                <div>
                  <label className="text-xs text-slate-400 block mb-1">Width</label>
                  <input
                    type="number"
                    value={width}
                    onChange={(e) => setWidth(Number(e.target.value))}
                    className="w-full bg-slate-900 border border-slate-600 rounded-lg px-3 py-2 text-sm focus:ring-2 focus:ring-cyan-500 focus:border-transparent outline-none"
                  />
                </div>
                <div>
                  <label className="text-xs text-slate-400 block mb-1">Height</label>
                  <input
                    type="number"
                    value={height}
                    onChange={(e) => setHeight(Number(e.target.value))}
                    className="w-full bg-slate-900 border border-slate-600 rounded-lg px-3 py-2 text-sm focus:ring-2 focus:ring-cyan-500 focus:border-transparent outline-none"
                  />
                </div>
                <div>
                  <label className="text-xs text-slate-400 block mb-1">FPS</label>
                  <input
                    type="number"
                    value={fps}
                    onChange={(e) => setFps(Number(e.target.value))}
                    className="w-full bg-slate-900 border border-slate-600 rounded-lg px-3 py-2 text-sm focus:ring-2 focus:ring-cyan-500 focus:border-transparent outline-none"
                  />
                </div>
                <div>
                  <label className="text-xs text-slate-400 block mb-1">Quality</label>
                  <input
                    type="number"
                    value={quality}
                    onChange={(e) => setQuality(Number(e.target.value))}
                    min={30}
                    max={95}
                    className="w-full bg-slate-900 border border-slate-600 rounded-lg px-3 py-2 text-sm focus:ring-2 focus:ring-cyan-500 focus:border-transparent outline-none"
                  />
                </div>
              </div>
            </div>
          </div>

          {/* Main content - Stream viewer */}
          <div className="lg:col-span-3 space-y-4">
            {/* Controls bar */}
            <div className="bg-slate-800/50 rounded-xl p-4 border border-slate-700 flex flex-wrap items-center gap-3">
              <button
                onClick={handleStart}
                disabled={status === 'running'}
                className="bg-green-600 hover:bg-green-500 disabled:bg-green-600/50 disabled:cursor-not-allowed text-white rounded-lg px-6 py-2.5 font-medium transition-colors flex items-center gap-2"
              >
                <svg className="w-4 h-4" fill="currentColor" viewBox="0 0 20 20"><path d="M6.3 2.841A1.5 1.5 0 004 4.11V15.89a1.5 1.5 0 002.3 1.269l9.344-5.89a1.5 1.5 0 000-2.538L6.3 2.84z"/></svg>
                Start
              </button>
              <button
                onClick={handleStop}
                disabled={status !== 'running'}
                className="bg-red-600 hover:bg-red-500 disabled:bg-red-600/50 disabled:cursor-not-allowed text-white rounded-lg px-6 py-2.5 font-medium transition-colors flex items-center gap-2"
              >
                <svg className="w-4 h-4" fill="currentColor" viewBox="0 0 20 20"><path d="M5.25 3A2.25 2.25 0 003 5.25v9.5A2.25 2.25 0 005.25 17h9.5A2.25 2.25 0 0017 14.75v-9.5A2.25 2.25 0 0014.75 3h-9.5z"/></svg>
                Stop
              </button>
              <div className="h-8 w-px bg-slate-600" />
              <button
                onClick={() => setRemoteControl(!remoteControl)}
                className={`rounded-lg px-4 py-2.5 font-medium transition-colors flex items-center gap-2 ${
                  remoteControl ? 'bg-cyan-600 hover:bg-cyan-500 text-white' : 'bg-slate-700 hover:bg-slate-600 text-slate-300'
                }`}
              >
                <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 15l-2 5L9 9l11 4-5 2zm0 0l5 5M7.188 2.239l.777 2.897M5.136 7.965l-2.898-.777M13.95 4.05l-2.122 2.122m-5.657 5.656l-2.12 2.122"/></svg>
                Remote Control {remoteControl ? 'ON' : 'OFF'}
              </button>
              <div className="h-8 w-px bg-slate-600" />
              <div className="flex items-center gap-2">
                <span className="text-sm text-slate-400">Consent:</span>
                <button
                  onClick={handleAllow}
                  className={`rounded-lg px-3 py-1.5 text-sm font-medium transition-colors ${
                    consentAllowed ? 'bg-green-600 text-white' : 'bg-slate-700 hover:bg-slate-600 text-slate-300'
                  }`}
                >
                  Allow
                </button>
                <button
                  onClick={handleDeny}
                  className={`rounded-lg px-3 py-1.5 text-sm font-medium transition-colors ${
                    !consentAllowed ? 'bg-red-600 text-white' : 'bg-slate-700 hover:bg-slate-600 text-slate-300'
                  }`}
                >
                  Deny
                </button>
              </div>
            </div>

            {/* Stream viewer */}
            <div
              ref={containerRef}
              className={`relative bg-black rounded-xl overflow-hidden border border-slate-700 ${remoteControl ? 'cursor-crosshair' : ''}`}
              style={{ aspectRatio: `${width}/${height}` }}
              tabIndex={0}
              onMouseDown={(e) => {
                if (!remoteControl || !imgRef.current) return;
                const [x, y] = mapCoords(e.clientX, e.clientY);
                const btn = e.button === 0 ? 'left' : e.button === 1 ? 'middle' : 'right';
                sendMouse({ x, y, button: btn, down: true, frame_w: width, frame_h: height, display_id: selectedDisplay });
                e.preventDefault();
              }}
              onMouseUp={(e) => {
                if (!remoteControl || !imgRef.current) return;
                const [x, y] = mapCoords(e.clientX, e.clientY);
                const btn = e.button === 0 ? 'left' : e.button === 1 ? 'middle' : 'right';
                sendMouse({ x, y, button: btn, down: false, frame_w: width, frame_h: height, display_id: selectedDisplay });
                e.preventDefault();
              }}
              onMouseMove={(e) => {
                if (!remoteControl || !imgRef.current) return;
                const [x, y] = mapCoords(e.clientX, e.clientY);
                sendMouse({ x, y, frame_w: width, frame_h: height, display_id: selectedDisplay });
              }}
              onWheel={(e) => {
                if (!remoteControl) return;
                sendMouse({ wheel_x: e.deltaX, wheel_y: e.deltaY, frame_w: width, frame_h: height, display_id: selectedDisplay });
                e.preventDefault();
              }}
              onKeyDown={(e) => {
                if (!remoteControl) return;
                const text = e.key.length === 1 ? e.key : undefined;
                sendKey({ key: e.key, text, down: true });
                e.preventDefault();
              }}
              onKeyUp={(e) => {
                if (!remoteControl) return;
                const text = e.key.length === 1 ? e.key : undefined;
                sendKey({ key: e.key, text, down: false });
                e.preventDefault();
              }}
              onContextMenu={(e) => e.preventDefault()}
            >
              {status === 'running' ? (
                <img
                  ref={imgRef}
                  src={streamUrl}
                  alt="Remote desktop stream"
                  className="w-full h-full object-contain"
                />
              ) : (
                <div className="absolute inset-0 flex flex-col items-center justify-center text-slate-500">
                  <svg className="w-16 h-16 mb-4 opacity-50" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M9.75 17L9 20l-1 1h8l-1-1-.75-3M3 13h18M5 17h14a2 2 0 002-2V5a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z"/>
                  </svg>
                  <p className="text-lg font-medium">Stream Offline</p>
                  <p className="text-sm">Click Start to begin streaming</p>
                </div>
              )}
              {!remoteControl && status === 'running' && (
                <div className="absolute inset-0 bg-black/30 flex items-center justify-center pointer-events-none">
                  <div className="bg-slate-900/90 backdrop-blur-sm px-6 py-3 rounded-lg border border-slate-600 text-sm">
                    Remote Control is OFF
                  </div>
                </div>
              )}
            </div>

            {/* Clipboard */}
            <div className="bg-slate-800/50 rounded-xl p-4 border border-slate-700">
              <h2 className="text-sm font-semibold text-slate-300 uppercase tracking-wide mb-3">Clipboard Sync</h2>
              <div className="flex gap-3">
                <textarea
                  value={clipboard}
                  onChange={(e) => setClipboard(e.target.value)}
                  className="flex-1 bg-slate-900 border border-slate-600 rounded-lg px-3 py-2 text-sm focus:ring-2 focus:ring-cyan-500 focus:border-transparent outline-none resize-none"
                  rows={2}
                  placeholder="Clipboard content..."
                />
                <div className="flex flex-col gap-2">
                  <button
                    onClick={handleReadClipboard}
                    className="bg-slate-700 hover:bg-slate-600 text-white rounded-lg px-4 py-2 text-sm font-medium transition-colors"
                  >
                    Read
                  </button>
                  <button
                    onClick={handleWriteClipboard}
                    className="bg-cyan-600 hover:bg-cyan-500 text-white rounded-lg px-4 py-2 text-sm font-medium transition-colors"
                  >
                    Write
                  </button>
                </div>
              </div>
            </div>

            {/* QUIC Status */}
            {ctrlStatus && (
              <div className="bg-slate-800/50 rounded-xl p-4 border border-slate-700">
                <h2 className="text-sm font-semibold text-slate-300 uppercase tracking-wide mb-3">QUIC Control Channel</h2>
                <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
                  <div className="bg-slate-900/50 rounded-lg p-3">
                    <div className="text-xs text-slate-500">Status</div>
                    <div className={`text-lg font-semibold ${ctrlStatus.connected ? 'text-green-400' : 'text-red-400'}`}>
                      {ctrlStatus.connected ? 'Connected' : 'Disconnected'}
                    </div>
                  </div>
                  <div className="bg-slate-900/50 rounded-lg p-3">
                    <div className="text-xs text-slate-500">Reconnects</div>
                    <div className="text-lg font-semibold text-white">{ctrlStatus.reconnects}</div>
                  </div>
                  <div className="bg-slate-900/50 rounded-lg p-3">
                    <div className="text-xs text-slate-500">Attempts</div>
                    <div className="text-lg font-semibold text-white">{ctrlStatus.attempts}</div>
                  </div>
                  <div className="bg-slate-900/50 rounded-lg p-3">
                    <div className="text-xs text-slate-500">Ping Interval</div>
                    <div className="text-lg font-semibold text-white">{ctrlStatus.ping_interval_secs || '-'}s</div>
                  </div>
                </div>
                {ctrlStatus.last_error && (
                  <div className="mt-3 p-3 bg-red-500/10 border border-red-500/30 rounded-lg">
                    <div className="text-xs text-red-400 font-medium">Last Error</div>
                    <div className="text-sm text-red-300">{ctrlStatus.last_error}</div>
                  </div>
                )}
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}

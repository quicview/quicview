'use client';

import { useState, useEffect, useCallback, useRef } from 'react';

type ViewMode = 'login' | 'desktop' | 'terminal';

interface ServerInfo {
  mode: 'desktop' | 'terminal';
  has_display: boolean;
}

export default function Home() {
  // View state
  const [view, setView] = useState<ViewMode>('login');
  const [serverInfo, setServerInfo] = useState<ServerInfo | null>(null);
  
  // Connection
  const [server, setServer] = useState('');
  const [token, setToken] = useState('');
  const [connecting, setConnecting] = useState(false);
  const [error, setError] = useState('');
  
  // Session state
  const [connected, setConnected] = useState(false);
  const [controlsVisible, setControlsVisible] = useState(false);
  const [fullscreen, setFullscreen] = useState(false);
  
  // Stream settings (from config, not shown to user)
  const [width, setWidth] = useState(1920);
  const [height, setHeight] = useState(1080);
  const [fps] = useState(30);
  const [quality] = useState(80);
  
  // Refs
  const imgRef = useRef<HTMLImageElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const controlsTimeout = useRef<NodeJS.Timeout | null>(null);

  // Load saved connection
  useEffect(() => {
    if (typeof window !== 'undefined') {
      const saved = localStorage.getItem('quicview.connection');
      if (saved) {
        try {
          const s = JSON.parse(saved);
          if (s.server) setServer(s.server);
          if (s.token) setToken(s.token);
        } catch {}
      }
    }
  }, []);

  // Fit to window
  useEffect(() => {
    const updateSize = () => {
      setWidth(window.innerWidth);
      setHeight(window.innerHeight);
    };
    updateSize();
    window.addEventListener('resize', updateSize);
    return () => window.removeEventListener('resize', updateSize);
  }, []);

  // Connect to server
  const connect = async () => {
    if (!server) {
      setError('Enter server address');
      return;
    }
    
    setConnecting(true);
    setError('');
    
    try {
      // Normalize URL
      let url = server;
      if (!url.startsWith('http')) url = `http://${url}`;
      if (!url.includes(':')) url = `${url}:8082`;
      
      const headers: HeadersInit = {};
      if (token) headers['Authorization'] = `Bearer ${token}`;
      
      // Check server status
      const res = await fetch(`${url}/status`, { headers });
      if (!res.ok) {
        if (res.status === 401) throw new Error('Invalid token');
        throw new Error('Connection failed');
      }
      
      const data = await res.json();
      
      // Determine mode based on server capabilities
      const hasDisplay = data.has_display ?? true;
      const mode = hasDisplay ? 'desktop' : 'terminal';
      
      setServerInfo({ mode, has_display: hasDisplay });
      
      // Save connection
      localStorage.setItem('quicview.connection', JSON.stringify({ server: url, token }));
      setServer(url);
      
      // Start streaming if not already
      if (!data.running) {
        await fetch(`${url}/start`, { method: 'POST', headers });
      }
      
      setConnected(true);
      setView(mode);
      
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Connection failed');
    } finally {
      setConnecting(false);
    }
  };

  // Disconnect
  const disconnect = useCallback(() => {
    setConnected(false);
    setView('login');
    setServerInfo(null);
  }, []);

  // Show controls on mouse move (auto-hide)
  const showControls = useCallback(() => {
    setControlsVisible(true);
    if (controlsTimeout.current) clearTimeout(controlsTimeout.current);
    controlsTimeout.current = setTimeout(() => setControlsVisible(false), 3000);
  }, []);

  // Toggle fullscreen
  const toggleFullscreen = useCallback(() => {
    if (!document.fullscreenElement) {
      containerRef.current?.requestFullscreen();
      setFullscreen(true);
    } else {
      document.exitFullscreen();
      setFullscreen(false);
    }
  }, []);

  // Input handlers
  const headers = useCallback((): HeadersInit => {
    const h: HeadersInit = { 'Content-Type': 'application/json' };
    if (token) h['Authorization'] = `Bearer ${token}`;
    return h;
  }, [token]);

  const sendMouse = useCallback(async (payload: object) => {
    try {
      await fetch(`${server}/input/mouse`, { method: 'POST', headers: headers(), body: JSON.stringify(payload) });
    } catch {}
  }, [server, headers]);

  const sendKey = useCallback(async (payload: object) => {
    try {
      await fetch(`${server}/input/key`, { method: 'POST', headers: headers(), body: JSON.stringify(payload) });
    } catch {}
  }, [server, headers]);

  const mapCoords = useCallback((clientX: number, clientY: number): [number, number] => {
    if (!imgRef.current) return [0, 0];
    const rect = imgRef.current.getBoundingClientRect();
    const x = ((clientX - rect.left) / rect.width) * width;
    const y = ((clientY - rect.top) / rect.height) * height;
    return [x, y];
  }, [width, height]);

  // Stream URL
  const streamUrl = `${server}/stream.mjpeg?w=${width}&h=${height}&fps=${fps}&q=${quality}${token ? `&token=${encodeURIComponent(token)}` : ''}`;

  // ============ LOGIN SCREEN ============
  if (view === 'login') {
    return (
      <div className="min-h-screen bg-[#0a0a0a] flex items-center justify-center p-4">
        <div className="w-full max-w-sm">
          {/* Logo */}
          <div className="text-center mb-8">
            <div className="w-16 h-16 bg-gradient-to-br from-blue-500 to-cyan-400 rounded-2xl mx-auto mb-4 flex items-center justify-center shadow-lg shadow-blue-500/20">
              <svg className="w-8 h-8 text-white" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9.75 17L9 20l-1 1h8l-1-1-.75-3M3 13h18M5 17h14a2 2 0 002-2V5a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z"/>
              </svg>
            </div>
            <h1 className="text-2xl font-semibold text-white">QuicView</h1>
            <p className="text-sm text-gray-500 mt-1">Remote Desktop Access</p>
          </div>

          {/* Login Form */}
          <div className="bg-[#111] rounded-2xl p-6 border border-gray-800">
            <div className="space-y-4">
              <div>
                <label className="text-xs text-gray-400 block mb-2">Server</label>
                <input
                  type="text"
                  value={server}
                  onChange={(e) => setServer(e.target.value)}
                  onKeyDown={(e) => e.key === 'Enter' && connect()}
                  className="w-full bg-black border border-gray-700 rounded-lg px-4 py-3 text-white placeholder-gray-600 focus:border-blue-500 focus:ring-1 focus:ring-blue-500 outline-none transition-colors"
                  placeholder="192.168.1.100:8082"
                  autoFocus
                />
              </div>
              <div>
                <label className="text-xs text-gray-400 block mb-2">Token <span className="text-gray-600">(optional)</span></label>
                <input
                  type="password"
                  value={token}
                  onChange={(e) => setToken(e.target.value)}
                  onKeyDown={(e) => e.key === 'Enter' && connect()}
                  className="w-full bg-black border border-gray-700 rounded-lg px-4 py-3 text-white placeholder-gray-600 focus:border-blue-500 focus:ring-1 focus:ring-blue-500 outline-none transition-colors"
                  placeholder="••••••••"
                />
              </div>
              
              {error && (
                <div className="bg-red-500/10 border border-red-500/30 rounded-lg px-4 py-3 text-red-400 text-sm">
                  {error}
                </div>
              )}
              
              <button
                onClick={connect}
                disabled={connecting}
                className="w-full bg-blue-600 hover:bg-blue-500 disabled:bg-blue-600/50 text-white rounded-lg px-4 py-3 font-medium transition-colors flex items-center justify-center gap-2"
              >
                {connecting ? (
                  <>
                    <svg className="w-5 h-5 animate-spin" fill="none" viewBox="0 0 24 24">
                      <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4"/>
                      <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z"/>
                    </svg>
                    Connecting...
                  </>
                ) : (
                  <>
                    <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13 10V3L4 14h7v7l9-11h-7z"/>
                    </svg>
                    Connect
                  </>
                )}
              </button>
            </div>
          </div>

          <p className="text-center text-gray-600 text-xs mt-6">
            Secure connection over QUIC
          </p>
        </div>
      </div>
    );
  }

  // ============ DESKTOP VIEW ============
  if (view === 'desktop') {
    return (
      <div
        ref={containerRef}
        className="fixed inset-0 bg-black cursor-none"
        tabIndex={0}
        onMouseDown={(e) => {
          if (!imgRef.current) return;
          const [x, y] = mapCoords(e.clientX, e.clientY);
          const btn = e.button === 0 ? 'left' : e.button === 1 ? 'middle' : 'right';
          sendMouse({ x, y, button: btn, down: true, frame_w: width, frame_h: height });
          e.preventDefault();
        }}
        onMouseUp={(e) => {
          if (!imgRef.current) return;
          const [x, y] = mapCoords(e.clientX, e.clientY);
          const btn = e.button === 0 ? 'left' : e.button === 1 ? 'middle' : 'right';
          sendMouse({ x, y, button: btn, down: false, frame_w: width, frame_h: height });
          e.preventDefault();
        }}
        onMouseMove={(e) => {
          showControls();
          if (!imgRef.current) return;
          const [x, y] = mapCoords(e.clientX, e.clientY);
          sendMouse({ x, y, frame_w: width, frame_h: height });
        }}
        onWheel={(e) => {
          sendMouse({ wheel_x: e.deltaX, wheel_y: e.deltaY, frame_w: width, frame_h: height });
          e.preventDefault();
        }}
        onKeyDown={(e) => {
          const text = e.key.length === 1 ? e.key : undefined;
          sendKey({ key: e.key, text, down: true });
          e.preventDefault();
        }}
        onKeyUp={(e) => {
          const text = e.key.length === 1 ? e.key : undefined;
          sendKey({ key: e.key, text, down: false });
          e.preventDefault();
        }}
        onContextMenu={(e) => e.preventDefault()}
      >
        {/* Stream */}
        {connected && (
          <img
            ref={imgRef}
            src={streamUrl}
            alt=""
            className="w-full h-full object-contain"
            draggable={false}
          />
        )}

        {/* Floating Controls - appears on mouse move */}
        <div className={`fixed top-0 left-0 right-0 transition-all duration-300 ${controlsVisible ? 'opacity-100 translate-y-0' : 'opacity-0 -translate-y-full pointer-events-none'}`}>
          <div className="flex justify-center pt-2">
            <div className="bg-black/80 backdrop-blur-sm rounded-full px-2 py-1 flex items-center gap-1 border border-gray-700/50">
              {/* Connection indicator */}
              <div className="flex items-center gap-2 px-3 py-1.5 text-sm text-gray-400">
                <div className="w-2 h-2 rounded-full bg-green-500 animate-pulse" />
                <span className="text-white font-medium">{server.replace(/^https?:\/\//, '').split(':')[0]}</span>
              </div>
              
              <div className="w-px h-6 bg-gray-700" />
              
              {/* Fullscreen */}
              <button
                onClick={toggleFullscreen}
                className="p-2 hover:bg-white/10 rounded-full transition-colors"
                title={fullscreen ? 'Exit Fullscreen' : 'Fullscreen'}
              >
                {fullscreen ? (
                  <svg className="w-5 h-5 text-white" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 9V4.5M9 9H4.5M9 9L3.75 3.75M9 15v4.5M9 15H4.5M9 15l-5.25 5.25M15 9h4.5M15 9V4.5M15 9l5.25-5.25M15 15h4.5M15 15v4.5m0-4.5l5.25 5.25"/>
                  </svg>
                ) : (
                  <svg className="w-5 h-5 text-white" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M3.75 3.75v4.5m0-4.5h4.5m-4.5 0L9 9M3.75 20.25v-4.5m0 4.5h4.5m-4.5 0L9 15M20.25 3.75h-4.5m4.5 0v4.5m0-4.5L15 9m5.25 11.25h-4.5m4.5 0v-4.5m0 4.5L15 15"/>
                  </svg>
                )}
              </button>
              
              {/* Disconnect */}
              <button
                onClick={disconnect}
                className="p-2 hover:bg-red-500/20 rounded-full transition-colors"
                title="Disconnect"
              >
                <svg className="w-5 h-5 text-red-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12"/>
                </svg>
              </button>
            </div>
          </div>
        </div>

        {/* Keyboard hint */}
        <div className={`fixed bottom-4 left-1/2 -translate-x-1/2 transition-all duration-300 ${controlsVisible ? 'opacity-100' : 'opacity-0'}`}>
          <div className="bg-black/60 backdrop-blur-sm rounded-lg px-3 py-1.5 text-xs text-gray-400">
            Press <kbd className="bg-gray-700 px-1.5 py-0.5 rounded text-white mx-1">Esc</kbd> to show controls
          </div>
        </div>
      </div>
    );
  }

  // ============ TERMINAL VIEW ============
  if (view === 'terminal') {
    return (
      <div
        ref={containerRef}
        className="fixed inset-0 bg-black flex flex-col"
        onMouseMove={showControls}
      >
        {/* Top bar */}
        <div className={`flex items-center justify-between px-4 py-2 bg-[#1a1a1a] border-b border-gray-800 transition-all duration-300 ${controlsVisible ? 'opacity-100' : 'opacity-70'}`}>
          <div className="flex items-center gap-3">
            <div className="flex items-center gap-2">
              <div className="w-3 h-3 rounded-full bg-red-500" />
              <div className="w-3 h-3 rounded-full bg-yellow-500" />
              <div className="w-3 h-3 rounded-full bg-green-500" />
            </div>
            <span className="text-sm text-gray-400 font-mono">{server.replace(/^https?:\/\//, '')} — shell</span>
          </div>
          <div className="flex items-center gap-2">
            <button
              onClick={toggleFullscreen}
              className="p-1.5 hover:bg-white/10 rounded transition-colors"
            >
              <svg className="w-4 h-4 text-gray-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 8V4m0 0h4M4 4l5 5m11-1V4m0 0h-4m4 0l-5 5M4 16v4m0 0h4m-4 0l5-5m11 5l-5-5m5 5v-4m0 4h-4"/>
              </svg>
            </button>
            <button
              onClick={disconnect}
              className="p-1.5 hover:bg-red-500/20 rounded transition-colors"
            >
              <svg className="w-4 h-4 text-red-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12"/>
              </svg>
            </button>
          </div>
        </div>

        {/* Terminal content - shows server has no display message */}
        <div className="flex-1 p-4 font-mono text-sm overflow-auto">
          <div className="text-green-400">Connected to {server.replace(/^https?:\/\//, '')}</div>
          <div className="text-gray-500 mt-2">Server is running in headless mode (no display)</div>
          <div className="text-gray-500">Terminal access requires SSH or a PTY endpoint.</div>
          <div className="mt-4 text-gray-400">
            <span className="text-cyan-400">$</span> <span className="animate-pulse">_</span>
          </div>
        </div>
      </div>
    );
  }

  return null;
}

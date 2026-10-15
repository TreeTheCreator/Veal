// The Settings proxy rows (plan-18): mode, bypass, and a read-only row that
// shows exactly what a new pane would get, from the same Rust path as the
// spawn. The Details section is how the Windows lookup gets debugged.
import { useCallback, useEffect, useRef, useState } from "react";
import { resolveProxy, type ProxyResolved } from "../pty";
import { DEFAULT_BYPASS, parsePort, useSettings, type ProxyMode } from "../settings/store";

const MODES: Array<{ value: ProxyMode; label: string }> = [
  { value: "off", label: "Off" },
  { value: "manual", label: "Manual" },
  { value: "system", label: "Follow system" },
];

const HINT: Record<ProxyMode, string> = {
  off: "New panes get no proxy variables",
  manual: "Sets http_proxy and https_proxy to 127.0.0.1:port in new panes",
  system: "New panes get the proxy Windows uses (Settings → Network & internet → Proxy)",
};

/** Red never works for Claude Code; amber is set but not exported. */
function stateOf(r: ProxyResolved): "ok" | "warn" | "bad" {
  if (r.socksOnly) return "bad";
  if (r.url === null && r.notes.length > 0) return "warn";
  return "ok";
}

export function ProxySettings() {
  const { proxyMode, proxyPort, proxyBypass, setProxyMode, setProxyPort, setProxyBypass } = useSettings();
  const portBad = proxyPort.trim() !== "" && parsePort(proxyPort) === null;
  const [resolved, setResolved] = useState<ProxyResolved | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [checking, setChecking] = useState(false);
  // Only the latest answer counts: typing a port fires one check per key.
  const seq = useRef(0);

  const check = useCallback(() => {
    const n = ++seq.current;
    setChecking(true);
    resolveProxy({ proxyMode, proxyPort, proxyBypass })
      .then((r) => {
        if (n !== seq.current) return;
        setResolved(r);
        setError(null);
      })
      .catch((e) => {
        if (n === seq.current) setError(String(e));
      })
      .finally(() => {
        if (n === seq.current) setChecking(false);
      });
  }, [proxyMode, proxyPort, proxyBypass]);

  useEffect(() => {
    const t = setTimeout(check, 250);
    return () => clearTimeout(t);
  }, [check]);

  const state = resolved ? stateOf(resolved) : "ok";
  const d = resolved?.diag;

  return (
    <>
      <div className="srow">
        <div className="lbl">
          <b>Proxy</b>
          <span>{proxyMode === "manual" && portBad ? "Not a valid port (1–65535); new panes start without a proxy" : HINT[proxyMode]}</span>
        </div>
        <div className="proxy-ctl">
          <div className="seg" role="radiogroup" aria-label="Proxy">
            {MODES.map((m) => (
              <button key={m.value} role="radio" aria-checked={proxyMode === m.value} className={proxyMode === m.value ? "on" : ""} onClick={() => setProxyMode(m.value)}>
                {m.label}
              </button>
            ))}
          </div>
          {proxyMode === "manual" && (
            <input
              className="mono port"
              inputMode="numeric"
              placeholder="Port"
              aria-label="Proxy port"
              value={proxyPort}
              aria-invalid={portBad}
              onChange={(e) => setProxyPort(e.target.value.replace(/\D/g, "").slice(0, 5))}
            />
          )}
        </div>
      </div>
      <div className="srow">
        <div className="lbl">
          <b>Bypass</b>
          <span>no_proxy, comma separated; wildcard and range support varies by tool</span>
        </div>
        <div className="proxy-ctl">
          <input className="mono bypass" placeholder="None" aria-label="Proxy bypass" value={proxyBypass} onChange={(e) => setProxyBypass(e.target.value)} />
          {proxyBypass !== DEFAULT_BYPASS && (
            <button className="btn" onClick={() => setProxyBypass(DEFAULT_BYPASS)} title={DEFAULT_BYPASS}>
              Default
            </button>
          )}
        </div>
      </div>
      <div className="srow proxy-now">
        <div className="lbl">
          <b>New panes get</b>
          <span>Open panes, and a running claude, keep the environment they started with</span>
        </div>
        <div className="proxy-result">
          <div className="proxy-line">
            <span className={`mono proxy-url ${state}`}>
              {error ? "Could not check" : resolved === null ? "Checking…" : `${resolved.source === "system" ? "Follow system" : resolved.source === "manual" ? "Manual" : "Off"} · ${resolved.url ?? "no proxy"}`}
            </span>
            <button className="btn" onClick={check} disabled={checking} title="Look up Windows' proxy again">
              {checking ? "Checking" : "Re-check"}
            </button>
          </div>
          {error && <span className="proxy-note bad">{error}</span>}
          {resolved?.notes.map((n) => (
            <span key={n} className={`proxy-note ${state}`}>{n}</span>
          ))}
        </div>
      </div>
      {resolved && d && (
        <details className="proxy-details">
          <summary>Details</summary>
          <dl>
            <dt>Variables</dt>
            <dd className="mono">{resolved.env.length ? resolved.env.map(([k, v]) => <div key={k}>{k}={v}</div>) : "none"}</dd>
            {d.lookup === null ? (
              <>
                <dt>Windows lookup</dt>
                <dd>Not read in this mode</dd>
              </>
            ) : (
              <>
                <dt>Windows lookup</dt>
                <dd className="mono">{LOOKUP_LABEL[d.lookup]}{d.lookupMs !== null && ` · ${d.lookupMs} ms`}{d.errorCode !== null && ` · error ${d.errorCode}`}</dd>
                <dt>Proxy as given</dt>
                <dd className="mono">{d.rawProxy ?? "none"}</dd>
                <dt>Bypass as given</dt>
                <dd className="mono">{d.rawBypass ?? "none"}{resolved.bypass.length > 0 && ` → ${resolved.bypass.join(",")}`}</dd>
                <dt>Setup script</dt>
                <dd className="mono">{d.scriptUrl ?? "none"}</dd>
              </>
            )}
            <dt>Host</dt>
            <dd className="mono">
              {d.host ?? "none"}
              {d.rawProxy?.toLowerCase().includes("localhost") && " (from localhost)"}
            </dd>
            <dt>Answered</dt>
            <dd className="mono">{d.answered ?? (d.lookup === "static" && resolved.url === null ? "nothing" : "not checked")}</dd>
          </dl>
          <p>
            localhost is exported as 127.0.0.1: Windows tries ::1 first, and a proxy that listens on IPv4 only then costs about
            200 ms per connection in curl. A loopback proxy is checked before it is exported; one that does not answer is left out.
          </p>
          <p>
            Claude's background agents run under a supervisor that outlives the shell and keeps the environment of the first shell
            that started it. For those, put the proxy in the <code>env</code> block of <code>~/.claude/settings.json</code>; Veal does
            not edit that file.
          </p>
        </details>
      )}
    </>
  );
}

const LOOKUP_LABEL: Record<NonNullable<ProxyResolved["diag"]["lookup"]>, string> = {
  static: "fixed proxy",
  script: "setup script only",
  autodetect: "automatic detection only",
  none: "no proxy set",
  unavailable: "failed",
};

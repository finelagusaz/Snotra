type TraceData = Record<string, unknown>;

const ENABLED =
  import.meta.env.DEV &&
  typeof window !== "undefined" &&
  window.localStorage.getItem("snotra_trace") === "1";

let seq = 0;

function nowMs(): number {
  return Math.round(performance.now() * 1000) / 1000;
}

export function trace(event: string, data: TraceData = {}): void {
  if (!ENABLED) return;
  seq += 1;
  const row = {
    ts: new Date().toISOString(),
    ms: nowMs(),
    seq,
    event,
    ...data,
  };
  console.debug(`[trace] ${JSON.stringify(row)}`);
}


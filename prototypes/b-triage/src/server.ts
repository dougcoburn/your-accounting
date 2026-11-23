import * as esbuild from "esbuild";
import { existsSync, readFileSync, statSync } from "node:fs";
import { createServer, type IncomingMessage, type ServerResponse } from "node:http";
import { extname, join, normalize, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parseMonarchCsv } from "./csv.ts";
import { booksFromObservations, progress } from "./ledger.ts";

/** Default for this prototype: ignore Monarch Reviewed so a fully-reviewed export is still triable. */
const DEFAULT_HONOR_MONARCH_REVIEWED = false;


const HOST = "127.0.0.1";
const PORT = 5172;

const root = resolve(fileURLToPath(new URL("..", import.meta.url)));
const publicDir = join(root, "public");
const defaultCsv = join(root, "data", "synthetic.csv");

const MIME: Record<string, string> = {
  ".html": "text/html; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".svg": "image/svg+xml",
  ".json": "application/json; charset=utf-8",
  ".ico": "image/x-icon",
};

type BootstrapSource = "synthetic" | "env";

function loadObservations(): {
  source: BootstrapSource;
  count: number;
  observations: ReturnType<typeof parseMonarchCsv>["observations"];
  issueCount: number;
} {
  const envPath = process.env.YA_MONARCH_CSV?.trim();
  const source: BootstrapSource = envPath ? "env" : "synthetic";
  const csvPath = envPath || defaultCsv;
  if (!existsSync(csvPath)) {
    const where = source === "env" ? "YA_MONARCH_CSV" : "synthetic fixture";
    throw new Error(`CSV not found (${where})`);
  }
  const text = readFileSync(csvPath, "utf8");
  const parsed = parseMonarchCsv(text);
  return {
    source,
    count: parsed.observations.length,
    observations: parsed.observations,
    issueCount: parsed.issues.length,
  };
}

async function bundleClient(): Promise<void> {
  await esbuild.build({
    absWorkingDir: root,
    entryPoints: [join(root, "src", "ui.ts")],
    bundle: true,
    outfile: join(publicDir, "app.js"),
    format: "iife",
    platform: "browser",
    target: "es2022",
    logLevel: "silent",
  });
}

function send(res: ServerResponse, status: number, body: string | Buffer, contentType: string): void {
  res.writeHead(status, {
    "Content-Type": contentType,
    "Cache-Control": "no-store",
    "X-Content-Type-Options": "nosniff",
  });
  res.end(body);
}

function safePublicFile(urlPath: string): string | undefined {
  const clean = decodeURIComponent(urlPath.split("?")[0] || "/");
  const rel = clean === "/" ? "index.html" : clean.replace(/^\/+/, "");
  const resolved = resolve(publicDir, normalize(rel));
  const relToPublic = relative(publicDir, resolved);
  if (relToPublic.startsWith("..") || relToPublic.includes("\0")) return undefined;
  if (!existsSync(resolved) || !statSync(resolved).isFile()) return undefined;
  return resolved;
}

function handle(req: IncomingMessage, res: ServerResponse, bootstrap: ReturnType<typeof loadObservations>): void {
  const url = req.url || "/";
  const method = req.method || "GET";
  if (method !== "GET" && method !== "HEAD") {
    send(res, 405, "Method Not Allowed\n", "text/plain; charset=utf-8");
    return;
  }
  if (url.split("?")[0] === "/health") {
    send(res, 200, "ok\n", "text/plain; charset=utf-8");
    return;
  }
  if (url.split("?")[0] === "/api/bootstrap") {
    // Counts only in the server log; the body is for the local browser.
    const start = progress(
      booksFromObservations(bootstrap.observations, {
        honorMonarchReviewed: DEFAULT_HONOR_MONARCH_REVIEWED,
      }),
    );
    const body = JSON.stringify({
      source: bootstrap.source,
      count: bootstrap.count,
      issueCount: bootstrap.issueCount,
      honorMonarchReviewed: DEFAULT_HONOR_MONARCH_REVIEWED,
      unreviewedCount: start.unreviewed,
      observations: bootstrap.observations,
    });
    send(res, 200, body, "application/json; charset=utf-8");
    return;
  }
  const file = safePublicFile(url);
  if (!file) {
    send(res, 404, "Not found\n", "text/plain; charset=utf-8");
    return;
  }
  const type = MIME[extname(file).toLowerCase()] || "application/octet-stream";
  const buf = readFileSync(file);
  if (method === "HEAD") {
    res.writeHead(200, { "Content-Type": type, "Content-Length": buf.length, "Cache-Control": "no-store" });
    res.end();
    return;
  }
  send(res, 200, buf, type);
}

async function main(): Promise<void> {
  await bundleClient();
  const bootstrap = loadObservations();
  const server = createServer((req, res) => {
    try {
      handle(req, res, bootstrap);
    } catch (err) {
      // Never include request bodies or row contents in logs.
      console.error("request failed", err instanceof Error ? err.message : "error");
      if (!res.headersSent) send(res, 500, "Internal error\n", "text/plain; charset=utf-8");
    }
  });
  server.listen(PORT, HOST, () => {
    const from = bootstrap.source === "env" ? "YA_MONARCH_CSV" : "synthetic fixture";
    console.log(`Inbox triage listening on http://${HOST}:${PORT}`);
    console.log(`Loaded ${bootstrap.count} observations from ${from}`);
    if (bootstrap.issueCount > 0) {
      console.log(`Parser reported ${bootstrap.issueCount} issue(s); those rows were skipped`);
    }
  });
}

main().catch((err) => {
  console.error("startup failed", err instanceof Error ? err.message : "error");
  process.exit(1);
});

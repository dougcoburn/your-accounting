import { createServer, type IncomingMessage, type ServerResponse } from "node:http";
import { readFile } from "node:fs/promises";
import { stripTypeScriptTypes } from "node:module";
import path from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = path.dirname(fileURLToPath(import.meta.url));
const HOST = "127.0.0.1";
const PORT = 5173;
const CSP = "default-src 'none'; style-src 'self'; script-src 'self'; connect-src 'self'; base-uri 'none'; form-action 'none'";

function log(message: string): void {
  process.stdout.write(`${message}\n`);
}

function logPath(pathname: string): string {
  if (pathname === "/" || pathname === "/styles.css" || pathname === "/api/source" || pathname === "/favicon.ico") {
    return pathname;
  }
  if (/^\/src\/[a-z][a-z0-9-]*\.ts$/.test(pathname)) {
    return pathname;
  }
  return "other";
}

function headers(type: string, length: number): Record<string, string | number> {
  return {
    "Content-Type": type,
    "Content-Length": length,
    "Cache-Control": "no-store",
    "Content-Security-Policy": CSP,
    "X-Content-Type-Options": "nosniff",
    "Referrer-Policy": "no-referrer",
  };
}

function send(res: ServerResponse, status: number, type: string, body: string | Buffer): void {
  const buffer = typeof body === "string" ? Buffer.from(body) : body;
  res.writeHead(status, headers(type, buffer.length));
  res.end(buffer);
}

async function loadSource(): Promise<{ source: "fixture" | "env"; csv: string; books: unknown }> {
  const envPath = process.env.YA_MONARCH_CSV?.trim();
  if (envPath) {
    const csv = await readFile(envPath, "utf8");
    return { source: "env", csv, books: null };
  }
  const csv = await readFile(path.join(ROOT, "fixtures", "synthetic-monarch.csv"), "utf8");
  const books = JSON.parse(await readFile(path.join(ROOT, "fixtures", "synthetic-books.json"), "utf8")) as unknown;
  return { source: "fixture", csv, books };
}

async function handle(req: IncomingMessage, res: ServerResponse): Promise<void> {
  if (req.method !== "GET") {
    send(res, 405, "text/plain; charset=utf-8", "Method not allowed");
    log(`${req.method ?? "?"} 405`);
    return;
  }
  let pathname = "/";
  try {
    pathname = new URL(req.url ?? "/", "http://127.0.0.1").pathname;
  } catch {
    send(res, 400, "text/plain; charset=utf-8", "Bad request");
    log("GET 400");
    return;
  }
  const label = logPath(pathname);
  try {
    if (pathname === "/favicon.ico") {
      res.writeHead(204, {
        "Cache-Control": "no-store",
        "Content-Security-Policy": CSP,
        "X-Content-Type-Options": "nosniff",
        "Referrer-Policy": "no-referrer",
      });
      res.end();
      log("GET /favicon.ico 204");
      return;
    }
    if (pathname === "/") {
      send(res, 200, "text/html; charset=utf-8", await readFile(path.join(ROOT, "index.html")));
      log("GET / 200");
      return;
    }
    if (pathname === "/styles.css") {
      send(res, 200, "text/css; charset=utf-8", await readFile(path.join(ROOT, "styles.css")));
      log("GET /styles.css 200");
      return;
    }
    if (pathname === "/api/source") {
      try {
        const payload = await loadSource();
        send(res, 200, "application/json; charset=utf-8", JSON.stringify({
          source: payload.source,
          csv: payload.csv,
          books: payload.books,
        }));
        log(`GET /api/source 200 ${payload.source}`);
      } catch {
        const message = process.env.YA_MONARCH_CSV?.trim()
          ? "YA_MONARCH_CSV could not be read"
          : "The synthetic fixture could not be read";
        send(res, 500, "application/json; charset=utf-8", JSON.stringify({ error: message }));
        log("GET /api/source 500");
      }
      return;
    }
    if (pathname.startsWith("/src/")) {
      const name = pathname.slice("/src/".length);
      if (!/^[a-z][a-z0-9-]*\.ts$/.test(name)) {
        send(res, 404, "text/plain; charset=utf-8", "Not found");
        log("GET other 404");
        return;
      }
      const source = await readFile(path.join(ROOT, "src", name), "utf8");
      const javascript = stripTypeScriptTypes(source, { mode: "strip" });
      send(res, 200, "application/javascript; charset=utf-8", javascript);
      log(`GET /src/${name} 200`);
      return;
    }
    send(res, 404, "text/plain; charset=utf-8", "Not found");
    log(`GET ${label} 404`);
  } catch {
    if (!res.headersSent) {
      send(res, 500, "text/plain; charset=utf-8", "Not available");
    }
    log(`GET ${label} 500`);
  }
}

const server = createServer((req, res) => {
  handle(req, res).catch(() => {
    if (!res.headersSent) {
      send(res, 500, "text/plain; charset=utf-8", "Not available");
    }
    log("GET 500");
  });
});

server.on("error", () => {
  log("prototype-c listen failed");
  process.exit(1);
});

server.listen(PORT, HOST, () => {
  const source = process.env.YA_MONARCH_CSV?.trim() ? "env" : "fixture";
  log(`prototype-c listening on http://127.0.0.1:${PORT} (${source})`);
});

#!/usr/bin/env node
/**
 * Loopback-only static server + in-memory row API for Prototype A.
 * Binds 127.0.0.1. Logs counts only — never row contents.
 */

import fs from "node:fs";
import http from "node:http";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { parseMonarchCsv } from "./lib/csv.mjs";
import { toRegisterLines } from "./lib/adapter.mjs";

const HOST = "127.0.0.1";
const PORT = Number.parseInt(process.env.PORT || "5171", 10);
const ROOT = path.dirname(fileURLToPath(import.meta.url));
const PUBLIC_DIR = path.join(ROOT, "public");
const LIB_DIR = path.join(ROOT, "lib");

const MIME = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".mjs": "text/javascript; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".svg": "image/svg+xml",
  ".json": "application/json; charset=utf-8",
  ".csv": "text/csv; charset=utf-8",
  ".ico": "image/x-icon",
  ".map": "application/json; charset=utf-8",
};

const CSP = [
  "default-src 'self'",
  "connect-src 'self'",
  "style-src 'self'",
  "script-src 'self'",
  "img-src 'self' data:",
  "font-src 'self'",
  "base-uri 'self'",
  "form-action 'self'",
  "frame-ancestors 'none'",
].join("; ");

function loadPayload() {
  const fromEnv = Boolean(process.env.YA_MONARCH_CSV);
  /** @type {string} */
  let text;
  /** @type {"synthetic" | "YA_MONARCH_CSV"} */
  const source = fromEnv ? "YA_MONARCH_CSV" : "synthetic";
  try {
    const filePath = fromEnv
      ? process.env.YA_MONARCH_CSV
      : path.join(ROOT, "data", "synthetic-october.csv");
    text = fs.readFileSync(filePath, "utf8");
  } catch (err) {
    const code = err && err.code ? err.code : "unknown";
    throw new Error(`failed to read ${fromEnv ? "YA_MONARCH_CSV" : "synthetic fixture"} (${code})`);
  }
  const records = parseMonarchCsv(text);
  const { lines, observationCount, expectedCount } = toRegisterLines(records, {
    includeSyntheticExpected: source === "synthetic",
  });
  return { source, lines, observationCount, expectedCount };
}

const payload = loadPayload();
console.log(
  `loaded ${payload.lines.length} lines (${payload.observationCount} observed, ${payload.expectedCount} expected) from ${payload.source}`,
);

/**
 * @param {string} urlPath
 * @param {string} baseDir
 * @returns {string | null}
 */
function safeFile(urlPath, baseDir) {
  const decoded = decodeURIComponent(urlPath);
  const relative = decoded.replace(/^\/+/, "");
  if (relative.includes("\0")) return null;
  const resolved = path.resolve(baseDir, relative);
  const base = path.resolve(baseDir) + path.sep;
  if (resolved !== path.resolve(baseDir) && !resolved.startsWith(base)) return null;
  return resolved;
}

/**
 * @param {http.ServerResponse} res
 * @param {number} status
 * @param {string} body
 * @param {string} [type]
 */
function send(res, status, body, type = "text/plain; charset=utf-8") {
  res.writeHead(status, {
    "Content-Type": type,
    "Content-Length": Buffer.byteLength(body),
    "Cache-Control": "no-store",
    "Content-Security-Policy": CSP,
    "X-Content-Type-Options": "nosniff",
    "Referrer-Policy": "no-referrer",
  });
  res.end(body);
}

/**
 * @param {http.ServerResponse} res
 * @param {string} filePath
 */
function sendFile(res, filePath) {
  fs.readFile(filePath, (err, data) => {
    if (err) {
      if (err.code === "ENOENT" || err.code === "EISDIR") {
        send(res, 404, "Not found");
        return;
      }
      send(res, 500, "Internal error");
      return;
    }
    const type = MIME[path.extname(filePath).toLowerCase()] || "application/octet-stream";
    res.writeHead(200, {
      "Content-Type": type,
      "Content-Length": data.length,
      "Cache-Control": "no-store",
      "Content-Security-Policy": CSP,
      "X-Content-Type-Options": "nosniff",
      "Referrer-Policy": "no-referrer",
    });
    res.end(data);
  });
}

const server = http.createServer((req, res) => {
  const method = req.method || "GET";
  const host = req.headers.host || `${HOST}:${PORT}`;
  let pathname = "/";
  try {
    pathname = new URL(req.url || "/", `http://${host}`).pathname;
  } catch {
    send(res, 400, "Bad request");
    return;
  }

  if (method !== "GET" && method !== "HEAD") {
    send(res, 405, "Method not allowed");
    return;
  }

  if (pathname === "/api/rows") {
    const body = JSON.stringify({
      source: payload.source,
      count: payload.lines.length,
      observationCount: payload.observationCount,
      expectedCount: payload.expectedCount,
      lines: payload.lines,
    });
    send(res, 200, body, "application/json; charset=utf-8");
    return;
  }

  if (pathname === "/") {
    sendFile(res, path.join(PUBLIC_DIR, "index.html"));
    return;
  }

  if (pathname.startsWith("/lib/")) {
    const filePath = safeFile(pathname.slice("/lib/".length), LIB_DIR);
    if (!filePath) {
      send(res, 403, "Forbidden");
      return;
    }
    sendFile(res, filePath);
    return;
  }

  const filePath = safeFile(pathname, PUBLIC_DIR);
  if (!filePath) {
    send(res, 403, "Forbidden");
    return;
  }
  sendFile(res, filePath);
});

server.listen(PORT, HOST, () => {
  console.log(`Prototype A register listening on http://${HOST}:${PORT}/`);
});

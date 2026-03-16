#!/usr/bin/env node
/** Wait until the backend HTTP API is reachable (port 11423). */
const url = "http://127.0.0.1:11423/api/settings";
const timeout = 120_000;
const interval = 500;
const start = Date.now();

process.stdout.write("Waiting for backend");
const timer = setInterval(async () => {
  try {
    await fetch(url);
    clearInterval(timer);
    process.stdout.write(" ready!\n");
    process.exit(0);
  } catch {
    if (Date.now() - start > timeout) {
      clearInterval(timer);
      process.stderr.write("\nBackend did not start within 2 minutes.\n");
      process.exit(1);
    }
    process.stdout.write(".");
  }
}, interval);

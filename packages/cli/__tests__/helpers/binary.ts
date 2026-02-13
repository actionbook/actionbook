import { execFile } from "child_process";
import { existsSync } from "fs";
import path from "path";

/**
 * Resolve the actionbook binary path.
 * Priority: ACTIONBOOK_BINARY_PATH env > cargo debug build > empty string (skip).
 */
export function getActionbookBinary(): string {
  if (process.env.ACTIONBOOK_BINARY_PATH) {
    return process.env.ACTIONBOOK_BINARY_PATH;
  }

  const cargoBinary = path.resolve(
    __dirname,
    "..",
    "..",
    "..",
    "actionbook-rs",
    "target",
    "debug",
    "actionbook"
  );
  if (existsSync(cargoBinary)) {
    return cargoBinary;
  }

  // Also check release build
  const releaseBinary = path.resolve(
    __dirname,
    "..",
    "..",
    "..",
    "actionbook-rs",
    "target",
    "release",
    "actionbook"
  );
  if (existsSync(releaseBinary)) {
    return releaseBinary;
  }

  return "";
}

export interface RunCliResult {
  stdout: string;
  stderr: string;
  exitCode: number | null;
}

/**
 * Run `node bin/actionbook.js` (the Node.js wrapper) with given args and env.
 * Used for testing the wrapper itself.
 */
export function runWrapper(
  args: string[] = [],
  options: {
    env?: Record<string, string>;
    timeout?: number;
  } = {}
): Promise<RunCliResult> {
  const wrapperPath = path.resolve(__dirname, "..", "..", "bin", "actionbook.js");
  const timeoutMs = options.timeout ?? 15000;

  return new Promise((resolve) => {
    const child = execFile(
      process.execPath,
      [wrapperPath, ...args],
      {
        env: { ...process.env, ...options.env },
        timeout: timeoutMs,
        maxBuffer: 10 * 1024 * 1024,
      },
      (error, stdout, stderr) => {
        const exitCode =
          error && "code" in error
            ? (error as NodeJS.ErrnoException & { code?: number }).code ??
              (error as any).status ??
              null
            : 0;

        resolve({
          stdout: stdout.toString(),
          stderr: stderr.toString(),
          exitCode:
            child.exitCode !== null ? child.exitCode : exitCode,
        });
      }
    );
  });
}

/**
 * Run the actionbook CLI binary with the given arguments.
 * Returns stdout, stderr, and exit code.
 */
export function runCli(
  args: string[],
  options: {
    env?: Record<string, string>;
    timeout?: number;
  } = {}
): Promise<RunCliResult> {
  const binary = getActionbookBinary();
  if (!binary) {
    return Promise.reject(new Error("Actionbook binary not found"));
  }

  const timeoutMs = options.timeout ?? 15000;

  return new Promise((resolve) => {
    const child = execFile(
      binary,
      args,
      {
        env: { ...process.env, ...options.env },
        timeout: timeoutMs,
        maxBuffer: 10 * 1024 * 1024,
      },
      (error, stdout, stderr) => {
        const exitCode =
          error && "code" in error
            ? (error as NodeJS.ErrnoException & { code?: number }).code ??
              (error as any).status ??
              null
            : 0;

        resolve({
          stdout: stdout.toString(),
          stderr: stderr.toString(),
          exitCode:
            child.exitCode !== null ? child.exitCode : exitCode,
        });
      }
    );
  });
}

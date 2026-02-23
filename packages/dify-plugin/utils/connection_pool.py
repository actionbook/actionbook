"""In-process pool of browser workers keyed by session_id.

Runtime note:
The Dify plugin process may run inside an active asyncio loop where
Playwright sync API is not usable. To avoid that, each session is executed
in a dedicated child Python process (utils/playwright_worker.py).
"""

import atexit
import json
import logging
import subprocess
import sys
import threading
import time
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

logger = logging.getLogger(__name__)


class ConnectionNotFound(Exception):
    """Raised when no pooled connection exists for the given session_id."""


class ConnectionUnhealthy(Exception):
    """Raised when a pooled connection worker is no longer usable."""


class WorkerTimeoutError(Exception):
    """Raised when a worker operation times out (maps to PlaywrightTimeout)."""


class _KeyboardProxy:
    def __init__(self, worker: "_SubprocessWorker") -> None:
        self._worker = worker

    def press(self, key: str) -> Any:
        return self._worker.call("keyboard_press", {"key": key})


class _PooledPageProxy:
    """Playwright Page-like proxy backed by a subprocess worker."""

    def __init__(self, worker: "_SubprocessWorker") -> None:
        self._worker = worker
        self.keyboard = _KeyboardProxy(worker)

    @property
    def url(self) -> str:
        return str(self._worker.call("url", {}))

    def title(self) -> str:
        return str(self._worker.call("title", {}))

    def goto(self, url: str, timeout: float | None = None, wait_until: str | None = None) -> Any:
        return self._worker.call("goto", {
            "url": url,
            "timeout": timeout,
            "wait_until": wait_until,
        })

    def wait_for_selector(self, selector: str, timeout: float | None = None) -> Any:
        return self._worker.call("wait_for_selector", {
            "selector": selector,
            "timeout": timeout,
        })

    def click(self, selector: str) -> Any:
        return self._worker.call("click", {"selector": selector})

    def type(self, selector: str, text: str) -> Any:
        return self._worker.call("type", {"selector": selector, "text": text})

    def fill(self, selector: str, text: str) -> Any:
        return self._worker.call("fill", {"selector": selector, "text": text})

    def select_option(self, selector: str, value: str) -> Any:
        return self._worker.call("select_option", {"selector": selector, "value": value})

    def hover(self, selector: str) -> Any:
        return self._worker.call("hover", {"selector": selector})

    def inner_text(self, selector: str) -> str:
        return str(self._worker.call("inner_text", {"selector": selector}))

    def inner_html(self, selector: str) -> str:
        return str(self._worker.call("inner_html", {"selector": selector}))

    def content(self) -> str:
        return str(self._worker.call("content", {}))

    def wait_for_load_state(self, state: str, timeout: float | None = None) -> Any:
        return self._worker.call("wait_for_load_state", {
            "state": state,
            "timeout": timeout,
        })

    def go_back(self) -> Any:
        return self._worker.call("go_back", {})

    def go_forward(self) -> Any:
        return self._worker.call("go_forward", {})

    def reload(self, wait_until: str | None = None) -> Any:
        return self._worker.call("reload", {"wait_until": wait_until})

    def evaluate(self, expression: str) -> Any:
        return self._worker.call("evaluate", {"expression": expression})


class _SubprocessWorker:
    """Owns a single Playwright session in a child process."""

    def __init__(self, session_id: str, ws_endpoint: str, timeout_ms: int = 30000) -> None:
        self._session_id = session_id
        self._timeout_ms = timeout_ms
        self._next_id = 1
        self._io_lock = threading.Lock()
        self._proc = self._start_worker(ws_endpoint, timeout_ms)
        self._handshake()

    def _start_worker(self, ws_endpoint: str, timeout_ms: int) -> subprocess.Popen[str]:
        worker_script = Path(__file__).with_name("playwright_worker.py")
        cmd = [
            sys.executable,
            str(worker_script),
            ws_endpoint,
            str(timeout_ms),
        ]
        return subprocess.Popen(
            cmd,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            text=True,
            bufsize=1,
        )

    def _handshake(self) -> None:
        resp = self._read_response_line()
        if not resp.get("ok"):
            err = resp.get("error") or "unknown startup failure"
            raise ConnectionUnhealthy(f"Worker startup failed for session '{self._session_id}': {err}")

    def call(self, op: str, params: dict[str, Any]) -> Any:
        if self._proc.poll() is not None:
            raise ConnectionUnhealthy(
                f"Browser worker exited for session '{self._session_id}' "
                f"(code={self._proc.returncode})."
            )

        with self._io_lock:
            req_id = self._next_id
            self._next_id += 1
            payload = {"id": req_id, "op": op, "params": params}
            self._write_payload(payload)
            resp = self._read_response_line()

        if not resp.get("ok"):
            error = resp.get("error") or "worker command failed"
            # Detect Playwright timeout errors so callers can handle them
            # distinctly from other failures (e.g. show "element not found"
            # hints instead of generic errors).
            if "TimeoutError" in error or "Timeout " in error:
                raise WorkerTimeoutError(error)
            raise RuntimeError(error)
        return resp.get("result")

    def stop(self) -> None:
        if self._proc.poll() is not None:
            return
        try:
            with self._io_lock:
                self._write_payload({"id": 0, "op": "close", "params": {}})
                # Do NOT read response — process may be mid-operation or
                # already exiting. Blocking here risks deadlock.
        except Exception:
            pass
        finally:
            # Close file descriptors to avoid resource leaks.
            for stream in (self._proc.stdin, self._proc.stdout):
                try:
                    if stream is not None:
                        stream.close()
                except Exception:
                    pass
            try:
                self._proc.terminate()
            except Exception:
                pass
            try:
                self._proc.wait(timeout=3)
            except Exception:
                try:
                    self._proc.kill()
                except Exception:
                    pass

    def _write_payload(self, payload: dict[str, Any]) -> None:
        if self._proc.stdin is None:
            raise ConnectionUnhealthy(f"Worker stdin unavailable for session '{self._session_id}'.")
        self._proc.stdin.write(json.dumps(payload, ensure_ascii=False) + "\n")
        self._proc.stdin.flush()

    def _read_response_line(self) -> dict[str, Any]:
        if self._proc.stdout is None:
            raise ConnectionUnhealthy(f"Worker stdout unavailable for session '{self._session_id}'.")

        line = self._proc.stdout.readline()
        if not line:
            raise ConnectionUnhealthy(
                f"Browser worker closed pipe for session '{self._session_id}'."
            )
        try:
            return json.loads(line)
        except json.JSONDecodeError as exc:
            raise ConnectionUnhealthy(
                f"Invalid worker response for session '{self._session_id}': {line[:200]}"
            ) from exc


@dataclass
class ManagedConnection:
    """A cached worker-backed CDP connection."""

    worker: _SubprocessWorker
    page: _PooledPageProxy
    session_id: str
    ws_endpoint: str
    provider_name: str = "hyperbrowser"
    api_key: str = ""
    created_at: float = field(default_factory=time.time)
    last_used_at: float = field(default_factory=time.time)

    def touch(self) -> None:
        self.last_used_at = time.time()


_DEFAULT_MAX_IDLE_SECONDS = 1800
_DEFAULT_MAX_POOL_SIZE = 20
_CLEANUP_INTERVAL_SECONDS = 300


class ConnectionPool:
    """Thread-safe pool of live browser workers keyed by session_id."""

    def __init__(
        self,
        max_idle_seconds: float = _DEFAULT_MAX_IDLE_SECONDS,
        max_size: int = _DEFAULT_MAX_POOL_SIZE,
    ) -> None:
        self._connections: dict[str, ManagedConnection] = {}
        self._lock = threading.Lock()
        self._max_idle_seconds = max_idle_seconds
        self._max_size = max_size
        self._start_cleanup_thread()

    def _start_cleanup_thread(self) -> None:
        def _loop() -> None:
            while True:
                time.sleep(_CLEANUP_INTERVAL_SECONDS)
                try:
                    self.cleanup_stale()
                except Exception:
                    logger.exception("Pool: cleanup_stale error")

        t = threading.Thread(target=_loop, daemon=True, name="pool-cleanup")
        t.start()

    def connect(
        self,
        session_id: str,
        ws_endpoint: str,
        timeout_ms: int = 30000,
        provider_name: str = "hyperbrowser",
        api_key: str = "",
    ) -> _PooledPageProxy:
        # Spawn worker outside the lock (expensive I/O operation).
        worker = _SubprocessWorker(
            session_id=session_id,
            ws_endpoint=ws_endpoint,
            timeout_ms=timeout_ms,
        )
        page = _PooledPageProxy(worker)
        conn = ManagedConnection(
            worker=worker,
            page=page,
            session_id=session_id,
            ws_endpoint=ws_endpoint,
            provider_name=provider_name,
            api_key=api_key,
        )

        evicted_conn: ManagedConnection | None = None
        with self._lock:
            # Evict any existing connection for this session_id (under lock).
            evicted_conn = self._connections.pop(session_id, None)

            if len(self._connections) >= self._max_size:
                # Kill the just-spawned worker before raising.
                worker.stop()
                raise RuntimeError(
                    f"Connection pool is full ({self._max_size} sessions). "
                    "Stop an existing session before creating a new one."
                )

            self._connections[session_id] = conn

        # Stop evicted worker outside lock to avoid deadlock.
        if evicted_conn is not None:
            _stop_worker_safely(evicted_conn.worker)
            logger.info("Pool: evicted previous connection for session")

        logger.info("Pool: connected session")
        return page

    _HEALTH_CHECK_INTERVAL = 30.0  # seconds idle before running a health check

    def get_page(self, session_id: str) -> _PooledPageProxy:
        with self._lock:
            conn = self._connections.get(session_id)
            if conn is None:
                raise ConnectionNotFound(
                    f"No pooled connection for session '{session_id}'. "
                    "Was browser_create_session called first?"
                )

        # Only health-check if the connection has been idle for a while,
        # or if the subprocess has exited.
        idle_seconds = time.time() - conn.last_used_at
        needs_check = (
            idle_seconds > self._HEALTH_CHECK_INTERVAL
            or conn.worker._proc.poll() is not None
        )
        if needs_check:
            try:
                conn.page.evaluate("1")
            except Exception as exc:
                # Remove from pool under lock, stop worker outside lock.
                with self._lock:
                    removed = self._connections.pop(session_id, None)
                if removed is not None:
                    _stop_worker_safely(removed.worker)
                    logger.info("Pool: disconnected session")
                raise ConnectionUnhealthy(
                    f"Pooled connection for session '{session_id}' is dead: {exc}"
                ) from exc

        conn.touch()
        return conn.page

    def disconnect(self, session_id: str) -> None:
        with self._lock:
            conn = self._connections.pop(session_id, None)
            if conn is None:
                logger.debug("Pool: disconnect called for unknown session")
                return
        # Stop worker outside lock to avoid deadlock with _io_lock.
        _stop_worker_safely(conn.worker)
        logger.info("Pool: disconnected session")

    def disconnect_all(self) -> None:
        with self._lock:
            conns = list(self._connections.values())
            self._connections.clear()
        # Stop all workers outside lock.
        for conn in conns:
            _stop_worker_safely(conn.worker)
        logger.info("Pool: disconnected all sessions")

    def cleanup_stale(self) -> None:
        now = time.time()
        with self._lock:
            stale = [
                sid
                for sid, conn in self._connections.items()
                if (now - conn.last_used_at) > self._max_idle_seconds
            ]
            stale_conns = [self._connections.pop(sid) for sid in stale]
        # Stop stale workers outside lock.
        for conn in stale_conns:
            logger.info("Pool: cleaning up stale session")
            _stop_worker_safely(conn.worker)

    @property
    def size(self) -> int:
        with self._lock:
            return len(self._connections)

    def has(self, session_id: str) -> bool:
        with self._lock:
            return session_id in self._connections

    def get_session_info(self, session_id: str) -> tuple[str, str] | None:
        """Return (provider_name, api_key) cached at create time, or None."""
        with self._lock:
            conn = self._connections.get(session_id)
            if conn is None:
                return None
            return (conn.provider_name, conn.api_key)

    # NOTE: _unsafe_disconnect was removed to fix a deadlock.
    # Worker shutdown now happens outside the pool lock via _stop_worker_safely().


def _stop_worker_safely(worker: _SubprocessWorker) -> None:
    """Stop a worker outside any pool lock to avoid deadlock with _io_lock."""
    try:
        worker.stop()
    except Exception as stop_err:
        logger.debug(
            "Pool: error stopping worker during disconnect (%s)",
            type(stop_err).__name__,
        )


pool = ConnectionPool()
atexit.register(pool.disconnect_all)

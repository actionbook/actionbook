"""Tests for the in-process subprocess-worker connection pool."""

import sys
import time
from pathlib import Path
from unittest.mock import MagicMock, patch

import pytest

sys.path.insert(0, str(Path(__file__).parent.parent))

from utils.connection_pool import (
    ConnectionNotFound,
    ConnectionPool,
    ConnectionUnhealthy,
)


def _make_mock_worker():
    """Create a mock _SubprocessWorker that passes health checks."""
    worker = MagicMock()
    worker._proc = MagicMock()
    worker._proc.poll.return_value = None  # process is alive
    worker.call.return_value = 1  # health check evaluate("1") returns 1
    return worker


@pytest.fixture
def fresh_pool():
    """Create a fresh ConnectionPool for each test."""
    return ConnectionPool(max_idle_seconds=60)


class TestConnect:
    @patch("utils.connection_pool._SubprocessWorker")
    def test_connect_returns_page(self, MockWorker, fresh_pool):
        mock_worker = _make_mock_worker()
        MockWorker.return_value = mock_worker

        result = fresh_pool.connect("sess-1", "wss://example.com/ws")

        assert fresh_pool.size == 1
        assert fresh_pool.has("sess-1")
        MockWorker.assert_called_once_with(
            session_id="sess-1",
            ws_endpoint="wss://example.com/ws",
            timeout_ms=30000,
        )

    @patch("utils.connection_pool._SubprocessWorker")
    def test_connect_replaces_existing(self, MockWorker, fresh_pool):
        worker1 = _make_mock_worker()
        worker2 = _make_mock_worker()
        MockWorker.side_effect = [worker1, worker2]

        fresh_pool.connect("sess-1", "wss://old.com/ws")
        fresh_pool.connect("sess-1", "wss://new.com/ws")

        assert fresh_pool.size == 1
        # Old worker should have been stopped
        worker1.stop.assert_called_once()

    @patch("utils.connection_pool._SubprocessWorker")
    def test_connect_failure_cleans_up(self, MockWorker, fresh_pool):
        MockWorker.side_effect = ConnectionUnhealthy("startup failed")

        with pytest.raises(ConnectionUnhealthy, match="startup failed"):
            fresh_pool.connect("sess-1", "wss://bad.com/ws")

        assert fresh_pool.size == 0

    @patch("utils.connection_pool._SubprocessWorker")
    def test_connect_custom_timeout(self, MockWorker, fresh_pool):
        mock_worker = _make_mock_worker()
        MockWorker.return_value = mock_worker

        fresh_pool.connect("sess-1", "wss://example.com/ws", timeout_ms=5000)

        MockWorker.assert_called_once_with(
            session_id="sess-1",
            ws_endpoint="wss://example.com/ws",
            timeout_ms=5000,
        )

    @patch("utils.connection_pool._SubprocessWorker")
    def test_connect_rejects_when_pool_full(self, MockWorker):
        pool = ConnectionPool(max_idle_seconds=60, max_size=2)
        MockWorker.return_value = _make_mock_worker()

        pool.connect("sess-1", "wss://a.com/ws")
        pool.connect("sess-2", "wss://b.com/ws")

        with pytest.raises(RuntimeError, match="full"):
            pool.connect("sess-3", "wss://c.com/ws")

        assert pool.size == 2


class TestGetPage:
    @patch("utils.connection_pool._SubprocessWorker")
    def test_get_page_returns_page_proxy(self, MockWorker, fresh_pool):
        mock_worker = _make_mock_worker()
        MockWorker.return_value = mock_worker

        fresh_pool.connect("sess-1", "wss://example.com/ws")
        result = fresh_pool.get_page("sess-1")

        # Should return a _PooledPageProxy
        assert result is not None

    def test_get_page_not_found(self, fresh_pool):
        with pytest.raises(ConnectionNotFound, match="sess-unknown"):
            fresh_pool.get_page("sess-unknown")

    @patch("utils.connection_pool._SubprocessWorker")
    def test_get_page_unhealthy_removes_connection(self, MockWorker, fresh_pool):
        mock_worker = _make_mock_worker()
        MockWorker.return_value = mock_worker

        fresh_pool.connect("sess-1", "wss://example.com/ws")

        # Simulate process exit so health check triggers
        mock_worker._proc.poll.return_value = 1  # exited
        mock_worker.call.side_effect = ConnectionUnhealthy("pipe closed")

        with pytest.raises(ConnectionUnhealthy, match="dead"):
            fresh_pool.get_page("sess-1")

        assert fresh_pool.size == 0

    @patch("utils.connection_pool._SubprocessWorker")
    def test_get_page_updates_last_used(self, MockWorker, fresh_pool):
        mock_worker = _make_mock_worker()
        MockWorker.return_value = mock_worker

        fresh_pool.connect("sess-1", "wss://example.com/ws")

        with fresh_pool._lock:
            conn = fresh_pool._connections["sess-1"]
            old_time = conn.last_used_at

        time.sleep(0.01)
        fresh_pool.get_page("sess-1")

        with fresh_pool._lock:
            conn = fresh_pool._connections["sess-1"]
            assert conn.last_used_at > old_time


class TestDisconnect:
    @patch("utils.connection_pool._SubprocessWorker")
    def test_disconnect_removes_connection(self, MockWorker, fresh_pool):
        mock_worker = _make_mock_worker()
        MockWorker.return_value = mock_worker

        fresh_pool.connect("sess-1", "wss://example.com/ws")
        fresh_pool.disconnect("sess-1")

        assert fresh_pool.size == 0
        assert not fresh_pool.has("sess-1")
        mock_worker.stop.assert_called_once()

    def test_disconnect_unknown_session_is_noop(self, fresh_pool):
        # Should not raise
        fresh_pool.disconnect("nonexistent")

    @patch("utils.connection_pool._SubprocessWorker")
    def test_disconnect_all(self, MockWorker, fresh_pool):
        worker1 = _make_mock_worker()
        worker2 = _make_mock_worker()
        MockWorker.side_effect = [worker1, worker2]

        fresh_pool.connect("sess-1", "wss://a.com/ws")
        fresh_pool.connect("sess-2", "wss://b.com/ws")
        assert fresh_pool.size == 2

        fresh_pool.disconnect_all()

        assert fresh_pool.size == 0
        worker1.stop.assert_called_once()
        worker2.stop.assert_called_once()


class TestCleanupStale:
    @patch("utils.connection_pool._SubprocessWorker")
    def test_cleanup_removes_stale_connections(self, MockWorker):
        pool = ConnectionPool(max_idle_seconds=0.01)  # 10ms TTL
        MockWorker.return_value = _make_mock_worker()

        pool.connect("sess-1", "wss://example.com/ws")
        time.sleep(0.02)  # Wait for it to become stale

        pool.cleanup_stale()

        assert pool.size == 0

    @patch("utils.connection_pool._SubprocessWorker")
    def test_cleanup_keeps_fresh_connections(self, MockWorker):
        pool = ConnectionPool(max_idle_seconds=60)
        MockWorker.return_value = _make_mock_worker()

        pool.connect("sess-1", "wss://example.com/ws")
        pool.cleanup_stale()

        assert pool.size == 1


class TestThreadSafety:
    @patch("utils.connection_pool._SubprocessWorker")
    def test_concurrent_connect_disconnect(self, MockWorker, fresh_pool):
        """Basic concurrency smoke test with multiple threads."""
        import threading

        MockWorker.return_value = _make_mock_worker()
        results = []

        def _worker(sid):
            try:
                fresh_pool.connect(sid, f"wss://example.com/{sid}")
                fresh_pool.get_page(sid)
                fresh_pool.disconnect(sid)
                results.append("ok")
            except Exception as e:
                results.append(str(e))

        threads = [threading.Thread(target=_worker, args=(f"s-{i}",)) for i in range(5)]
        for t in threads:
            t.start()
        for t in threads:
            t.join()

        # All threads should succeed (no deadlocks or crashes)
        assert len(results) == 5


class TestEdgeCases:
    @patch("utils.connection_pool._SubprocessWorker")
    def test_disconnect_handles_worker_stop_error(self, MockWorker, fresh_pool):
        mock_worker = _make_mock_worker()
        mock_worker.stop.side_effect = RuntimeError("already stopped")
        MockWorker.return_value = mock_worker

        fresh_pool.connect("sess-1", "wss://example.com/ws")
        # Should not raise
        fresh_pool.disconnect("sess-1")
        assert fresh_pool.size == 0

    @patch("utils.connection_pool._SubprocessWorker")
    def test_has_method(self, MockWorker, fresh_pool):
        MockWorker.return_value = _make_mock_worker()

        assert not fresh_pool.has("sess-1")
        fresh_pool.connect("sess-1", "wss://example.com/ws")
        assert fresh_pool.has("sess-1")
        fresh_pool.disconnect("sess-1")
        assert not fresh_pool.has("sess-1")

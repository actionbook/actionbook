"""Camoufox REST API Server - Real browser implementation."""

from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from contextlib import asynccontextmanager
import uvicorn

from models import *
from browser import CamofoxBrowserManager


# Global browser manager
manager: CamofoxBrowserManager


@asynccontextmanager
async def lifespan(app: FastAPI):
    """Application lifespan manager."""
    global manager
    manager = CamofoxBrowserManager()
    print("🦊 Camoufox REST API Server started")
    yield
    # Cleanup on shutdown
    await manager.cleanup()
    print("👋 Camoufox REST API Server stopped")


app = FastAPI(
    title="Camoufox REST API",
    version="0.6.2",
    description="Real Camoufox browser automation API",
    lifespan=lifespan,
)

# CORS middleware
app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)


@app.get("/health")
async def health_check():
    """Health check endpoint."""
    return {
        "server": "camoufox-rest-api",
        "status": "ok",
        "version": "0.6.2",
        "browser": "real" if manager.browser else "not_started"
    }


@app.post("/tabs", response_model=CreateTabResponse)
async def create_tab(request: CreateTabRequest):
    """Create a new browser tab and navigate to URL."""
    try:
        tab = await manager.create_tab(request.user_id, request.session_key, request.url)
        return CreateTabResponse(id=tab.id, url=tab.url)
    except HTTPException:
        raise
    except Exception as e:
        raise HTTPException(status_code=500, detail=f"Failed to create tab: {str(e)}")


@app.get("/tabs/{tab_id}/snapshot", response_model=SnapshotResponse)
async def get_snapshot(tab_id: str, user_id: str):
    """Get accessibility tree snapshot for tab."""
    try:
        tree = await manager.get_accessibility_tree(tab_id, user_id)
        return SnapshotResponse(tree=tree)
    except HTTPException:
        raise
    except Exception as e:
        raise HTTPException(status_code=500, detail=f"Failed to get snapshot: {str(e)}")


@app.post("/tabs/{tab_id}/click")
async def click_element(tab_id: str, request: ClickRequest):
    """Click element by element ref."""
    try:
        await manager.click(tab_id, request.user_id, request.element_ref)
        return {"success": True}
    except HTTPException:
        raise
    except Exception as e:
        raise HTTPException(status_code=500, detail=f"Click failed: {str(e)}")


@app.post("/tabs/{tab_id}/type")
async def type_text(tab_id: str, request: TypeTextRequest):
    """Type text into element."""
    try:
        await manager.type_text(tab_id, request.user_id, request.element_ref, request.text)
        return {"success": True}
    except HTTPException:
        raise
    except Exception as e:
        raise HTTPException(status_code=500, detail=f"Type failed: {str(e)}")


@app.post("/tabs/{tab_id}/navigate")
async def navigate(tab_id: str, request: NavigateRequest):
    """Navigate tab to URL."""
    try:
        await manager.navigate(tab_id, request.user_id, request.url)
        return {"success": True}
    except HTTPException:
        raise
    except Exception as e:
        raise HTTPException(status_code=500, detail=f"Navigation failed: {str(e)}")


@app.get("/tabs/{tab_id}/screenshot")
async def screenshot(tab_id: str, user_id: str):
    """Take screenshot and return base64 PNG."""
    try:
        png_base64 = await manager.screenshot(tab_id, user_id)
        return {"data": png_base64}
    except HTTPException:
        raise
    except Exception as e:
        raise HTTPException(status_code=500, detail=f"Screenshot failed: {str(e)}")


@app.get("/sessions/{session_key}/active-tab")
async def get_active_tab(session_key: str):
    """Get the active tab ID for a session."""
    tab_id = manager.get_active_tab_for_session(session_key)
    if not tab_id:
        raise HTTPException(
            status_code=404,
            detail=f"No active tab found for session: {session_key}"
        )
    return {"tab_id": tab_id}


def main():
    """Start the server."""
    print("=" * 50)
    print("🦊 Camoufox REST API Server")
    print("=" * 50)
    print("Version: 0.6.2")
    print("Address: http://0.0.0.0:9377")
    print("=" * 50)
    print("Endpoints:")
    print("  GET  /health                          - Health check")
    print("  POST /tabs                            - Create tab")
    print("  GET  /tabs/:id/snapshot               - Get accessibility tree")
    print("  POST /tabs/:id/click                  - Click element")
    print("  POST /tabs/:id/type                   - Type text")
    print("  POST /tabs/:id/navigate               - Navigate to URL")
    print("  GET  /tabs/:id/screenshot             - Take screenshot")
    print("  GET  /sessions/:session_key/active-tab - Get active tab for session")
    print("=" * 50)
    print("Press Ctrl+C to stop")
    print()

    uvicorn.run(
        app,
        host="0.0.0.0",
        port=9377,
        log_level="info",
    )


if __name__ == "__main__":
    main()

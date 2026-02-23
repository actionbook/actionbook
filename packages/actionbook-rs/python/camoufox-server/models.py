"""Pydantic models for Camoufox REST API."""

from pydantic import BaseModel, Field, ConfigDict
from typing import Optional, List


class CreateTabRequest(BaseModel):
    """Request to create a new tab."""
    model_config = ConfigDict(populate_by_name=True)

    user_id: str = Field(alias="userId")
    session_key: str = Field(alias="sessionKey")
    url: str


class CreateTabResponse(BaseModel):
    """Response with created tab info."""
    id: str
    url: str


class ClickRequest(BaseModel):
    """Request to click an element."""
    model_config = ConfigDict(populate_by_name=True)

    user_id: str = Field(alias="userId")
    element_ref: str = Field(alias="elementRef")


class TypeTextRequest(BaseModel):
    """Request to type text into element."""
    model_config = ConfigDict(populate_by_name=True)

    user_id: str = Field(alias="userId")
    element_ref: str = Field(alias="elementRef")
    text: str


class NavigateRequest(BaseModel):
    """Request to navigate to URL."""
    model_config = ConfigDict(populate_by_name=True)

    user_id: str = Field(alias="userId")
    url: str


class AccessibilityNode(BaseModel):
    """Accessibility tree node."""
    role: str
    name: Optional[str] = None
    element_ref: Optional[str] = None
    children: Optional[List['AccessibilityNode']] = None


class SnapshotResponse(BaseModel):
    """Response with accessibility tree snapshot."""
    tree: AccessibilityNode

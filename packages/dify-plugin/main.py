"""Actionbook Dify Plugin - Entry point for local/remote debugging."""

from dify_plugin import DifyPluginEnv, Plugin

plugin = Plugin(DifyPluginEnv())
plugin.run()

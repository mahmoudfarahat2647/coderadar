import React, { useState } from "react";
import { KeyRound, X } from "lucide-react";
import { EngineStatus } from "../types";
import { clearApiKey, storeApiKey } from "../services/tauri";

interface ApiKeyDialogProps {
  open: boolean;
  engineStatus: EngineStatus | null;
  onClose: () => void;
  onStatusChange: (status: EngineStatus) => void;
  onError: (message: string) => void;
}

/** Modal for setting/clearing the Jev OpenRouter API key (stored in the OS keychain). */
export const ApiKeyDialog: React.FC<ApiKeyDialogProps> = ({
  open,
  engineStatus,
  onClose,
  onStatusChange,
  onError,
}) => {
  const [apiKey, setApiKey] = useState("");
  const [busy, setBusy] = useState(false);

  if (!open) return null;

  const handleSave = async () => {
    if (!apiKey.trim() || busy) return;
    setBusy(true);
    try {
      const status = await storeApiKey(apiKey);
      onStatusChange(status);
      setApiKey("");
      onClose();
    } catch (err) {
      onError(typeof err === "string" ? err : String(err));
    } finally {
      setBusy(false);
    }
  };

  const handleClear = async () => {
    if (busy) return;
    setBusy(true);
    try {
      const status = await clearApiKey();
      onStatusChange(status);
      onClose();
    } catch (err) {
      onError(typeof err === "string" ? err : String(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div
      className="fixed inset-0 bg-black/60 z-50 flex items-center justify-center p-4"
      onClick={onClose}
    >
      <div
        className="w-full max-w-md bg-surface-panel border border-border-strong rounded-lg shadow-2xl"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="p-4 border-b border-border-subtle flex items-center justify-between">
          <div className="flex items-center space-x-2">
            <KeyRound className="w-4 h-4 text-secondary" />
            <span className="font-headline font-semibold text-sm text-text-primary">
              Jev API Key
            </span>
          </div>
          <button
            onClick={onClose}
            className="p-1 rounded-md text-text-muted hover:text-text-primary hover:bg-surface-elevated transition-colors"
            title="Close"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        <div className="p-4 space-y-3 text-xs">
          <p className="text-text-muted leading-relaxed">
            The key is stored in the OS keychain — never in a plaintext file.
            Every outbound chunk is Tier 0 redacted before it leaves this machine.
          </p>

          {engineStatus?.api_key_configured && (
            <div className="p-2.5 rounded-md bg-surface-elevated border border-border-subtle font-mono">
              <span className="text-text-muted">Stored: </span>
              <span className="text-secondary">{engineStatus.api_key_masked}</span>
            </div>
          )}

          <input
            type="password"
            value={apiKey}
            onChange={(e) => setApiKey(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") handleSave();
            }}
            placeholder="sk-or-v1-…"
            className="w-full bg-surface-elevated border border-border-subtle rounded-md px-3 py-2 font-mono text-xs text-text-primary placeholder:text-text-muted/60 focus:outline-none focus:border-secondary"
            autoFocus
          />

          <div className="flex items-center justify-between pt-1 space-x-2">
            {engineStatus?.api_key_configured ? (
              <button
                onClick={handleClear}
                disabled={busy}
                className="px-3 py-1.5 rounded-md border border-status-high/40 text-status-high hover:bg-status-high/10 transition-colors disabled:opacity-50"
              >
                Remove Key
              </button>
            ) : (
              <span />
            )}
            <button
              onClick={handleSave}
              disabled={busy || !apiKey.trim()}
              className="px-3 py-1.5 rounded-md bg-secondary/15 border border-secondary/40 text-secondary hover:bg-secondary/25 transition-colors disabled:opacity-50"
            >
              Save Key
            </button>
          </div>
        </div>
      </div>
    </div>
  );
};

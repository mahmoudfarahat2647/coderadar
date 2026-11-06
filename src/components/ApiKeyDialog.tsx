import React, { useEffect, useRef, useState } from "react";
import { KeyRound, X } from "lucide-react";
import { EngineStatus, ProviderDiagnostic, Tier2StatusDTO } from "../types";
import { checkProviderDiagnostic, clearApiKey, clearTier2Key, getTier2Status, storeApiKey, storeTier2Key } from "../services/tauri";
import { providerDiagnosticLabel } from "../utils/engineTone";

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
  const [tier2, setTier2] = useState<Tier2StatusDTO | null>(null);
  const [geminiKey, setGeminiKey] = useState("");
  const [tier2Busy, setTier2Busy] = useState(false);
  const [diagnostic, setDiagnostic] = useState<ProviderDiagnostic | null>(null);
  const dialogGeneration = useRef(0);

  // Phase 8: the Tier 2 provider state is read when the dialog opens.
  useEffect(() => {
    dialogGeneration.current += 1;
    if (!open) {
      setApiKey("");
      setGeminiKey("");
      setDiagnostic(null);
      return;
    }
    getTier2Status()
      .then((status) => setTier2(status))
      .catch(() => setTier2(null));
  }, [open]);

  if (!open) return null;

  const handleSave = async () => {
    if (!apiKey.trim() || busy) return;
    setBusy(true);
    try {
      const status = await storeApiKey(apiKey);
      onStatusChange(status);
      setApiKey("");
      onClose();
    } catch {
      onError("Could not save the API key.");
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
    } catch {
      onError("Could not remove the API key.");
    } finally {
      setBusy(false);
    }
  };

  const handleSaveGemini = async () => {
    if (!geminiKey.trim() || tier2Busy) return;
    setTier2Busy(true);
    try {
      const status = await storeTier2Key(geminiKey);
      setTier2(status);
      setGeminiKey("");
    } catch {
      onError("Could not save the Gemini key.");
    } finally {
      setTier2Busy(false);
    }
  };

  const handleClearGemini = async () => {
    if (tier2Busy) return;
    setTier2Busy(true);
    try {
      const status = await clearTier2Key();
      setTier2(status);
    } catch {
      onError("Could not remove the Gemini key.");
    } finally {
      setTier2Busy(false);
    }
  };

  const handleCheck = async (synthetic: boolean) => {
    if (busy) return;
    const generation = dialogGeneration.current;
    setBusy(true);
    setDiagnostic(null);
    try {
      const result = await checkProviderDiagnostic(synthetic);
      if (generation === dialogGeneration.current) setDiagnostic(result);
    } catch {
      if (generation === dialogGeneration.current) onError("Provider check could not be completed.");
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
              API Keys
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
              <span className="text-secondary">Key configured</span>
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

          <div className="pt-3 border-t border-border-subtle space-y-2">
            <p className="text-text-muted">Authentication sends no source code. The synthetic check sends a fixed test prompt, not workspace content, and may incur provider charges.</p>
            <div className="flex flex-wrap gap-2">
              <button type="button" disabled={busy} onClick={() => handleCheck(false)} className="px-3 py-1.5 rounded-md border border-border-subtle disabled:opacity-50">Check Authentication</button>
              <button type="button" disabled={busy} onClick={() => handleCheck(true)} className="px-3 py-1.5 rounded-md border border-border-subtle disabled:opacity-50">Run Synthetic Check</button>
            </div>
            {diagnostic && <p role="status" className="text-text-primary">{providerDiagnosticLabel(diagnostic.state)}{diagnostic.checked_at_unix_secs != null && ` Checked: ${new Date(diagnostic.checked_at_unix_secs * 1000).toLocaleString()}.`}</p>}
          </div>

          {/* Phase 8 - Tier 2 cloud fix provider */}
          <div className="pt-3 border-t border-border-subtle space-y-2">
            <div className="flex items-center justify-between">
              <span className="font-semibold text-text-primary text-[11px]">
                Tier 2 Cloud Fix (Gemini)
              </span>
              {tier2 && (
                <span className="font-mono text-[10px] px-1.5 py-0.5 rounded bg-surface-elevated border border-border-subtle text-text-muted">
                  {tier2.cloud_model}
                </span>
              )}
            </div>
            <p className="text-text-muted leading-relaxed">
              Used only when you explicitly choose Cloud Gemini for a generated
              fix. The backend refuses any file that cannot be fully redacted.
              Local Ollama fixes need no key and stay on this machine.
            </p>

            {tier2?.cloud_key_configured && (
              <div className="p-2.5 rounded-md bg-surface-elevated border border-border-subtle font-mono text-[11px]">
                <span className="text-text-muted">Stored: </span>
                <span className="text-secondary">Gemini key configured</span>
              </div>
            )}

            <input
              type="password"
              value={geminiKey}
              onChange={(e) => setGeminiKey(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") handleSaveGemini();
              }}
              placeholder="AIza..."
              className="w-full bg-surface-elevated border border-border-subtle rounded-md px-3 py-2 font-mono text-xs text-text-primary placeholder:text-text-muted/60 focus:outline-none focus:border-secondary"
            />

            <div className="flex items-center justify-between space-x-2">
              {tier2?.cloud_key_configured ? (
                <button
                  onClick={handleClearGemini}
                  disabled={tier2Busy}
                  className="px-3 py-1.5 rounded-md border border-status-high/40 text-status-high hover:bg-status-high/10 transition-colors disabled:opacity-50"
                >
                  Remove Key
                </button>
              ) : (
                <span />
              )}
              <button
                onClick={handleSaveGemini}
                disabled={tier2Busy || !geminiKey.trim()}
                className="px-3 py-1.5 rounded-md bg-secondary/15 border border-secondary/40 text-secondary hover:bg-secondary/25 transition-colors disabled:opacity-50"
              >
                Save Gemini Key
              </button>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
};

import { useState } from "react";
import { MdOutlinePrivacyTip } from "react-icons/md";
import type { PreparedEfficiencyPayload } from "../types";
import { PopoutModal } from "./PopoutModal";

interface EfficiencyPrivacyModalProps {
  payload: PreparedEfficiencyPayload;
  busy: boolean;
  onCancel: () => void;
  onConfirm: () => void;
}

export function EfficiencyPrivacyModal({
  payload,
  busy,
  onCancel,
  onConfirm,
}: EfficiencyPrivacyModalProps) {
  const [privacyConfirmed, setPrivacyConfirmed] = useState(false);

  return (
    <PopoutModal
      onClose={onCancel}
      header={
        <div className="efficiency-privacy__header">
          <span className="efficiency-privacy__header-icon" aria-hidden="true">
            <MdOutlinePrivacyTip />
          </span>
          <span>
            <h2>Privacy Notice</h2>
            <span>Before sharing session data with Jev</span>
          </span>
        </div>
      }
      initialWidth={620}
      initialHeight={720}
    >
      <div className="efficiency-privacy">
        <section className="efficiency-privacy__notice" aria-labelledby="privacy-notice-summary">
          <MdOutlinePrivacyTip aria-hidden="true" />
          <div>
            <h3 id="privacy-notice-summary">Your session data will leave this device</h3>
            <p>
              This analysis sends selected data from this Claude Code session to Jev, an external AI
              service.
            </p>
          </div>
        </section>
        <section className="efficiency-privacy__section">
          <h3>Where your data goes</h3>
          <dl className="efficiency-privacy__destination">
            <div>
              <dt>Provider</dt>
              <dd>{payload.destination.provider}</dd>
            </div>
            <div>
              <dt>API destination</dt>
              <dd>
                <code>{payload.destination.endpoint}</code>
              </dd>
            </div>
            <div>
              <dt>Model</dt>
              <dd>
                <code>{payload.destination.model}</code>
              </dd>
            </div>
            <div>
              <dt>Transport</dt>
              <dd>HTTPS; your Jev API key is sent as an authorisation credential</dd>
            </div>
          </dl>
          <p>
            TypeSafe AI operates Jev. Its current privacy policy says its services are hosted in the
            United States, personal data may be retained for as long as reasonably necessary, and no
            electronic transmission or storage can be guaranteed completely secure.
          </p>
          <p className="efficiency-privacy__links">
            Review TypeSafe AI&apos;s{" "}
            <a href="https://typesafe.ai/legal/privacy-policy">Privacy Policy</a>,{" "}
            <a href="https://typesafe.ai/legal/mca">service agreement</a>, and{" "}
            <a href="https://typesafe.ai/legal/data-processing">Data Processing Addendum</a> before
            sending protected or regulated data.
          </p>
        </section>
        <section className="efficiency-privacy__content">
          <h3>What may be shared</h3>
          <ul>
            <li>Prompts and responses</li>
            <li>Tool names, inputs, and results</li>
            <li>File paths and commands</li>
            <li>Repository or project information</li>
            <li>Other content present in the session transcript</li>
          </ul>
          <p>
            Claude Code Trace minimises and redacts the payload locally where possible, but
            sensitive information may still be included. Review the exact payload and your
            organisation&apos;s data-handling requirements before continuing.
          </p>
          <p className="efficiency-privacy__recurring-warning">
            This notice and confirmation are required every time an analysis, retry, or re-analysis
            sends a new request to Jev.
          </p>
        </section>
        <section className="efficiency-privacy__section">
          <h3>Your responsibility</h3>
          <ul>
            <li>Only send data you own or are authorised to disclose to TypeSafe AI.</li>
            <li>
              Do not send secrets, personal data, confidential information, customer data, source
              code, or regulated data unless you have assessed and accepted the applicable terms,
              safeguards, and legal requirements.
            </li>
            <li>
              Local minimisation and redaction are best-effort safeguards and may fail to identify
              every sensitive value or inference.
            </li>
            <li>
              Jev results are automated, may be inaccurate or incomplete, and are not legal,
              security, compliance, employment, or professional advice.
            </li>
          </ul>
        </section>
        <details className="efficiency-privacy__preview">
          <summary>Review the exact data being sent</summary>
          <pre>{JSON.stringify(payload.input, null, 2)}</pre>
        </details>
        <section className="efficiency-privacy__disclaimer" aria-labelledby="analysis-disclaimer">
          <h3 id="analysis-disclaimer">Important disclaimer</h3>
          <p>
            Claude Code Trace is independent open-source software and does not operate or control
            Jev or TypeSafe AI&apos;s services, security, storage, retention, subprocessors,
            policies, availability, or outputs. The software and this optional integration are
            provided
            <strong> as is</strong>, without warranties of any kind. To the maximum extent permitted
            by applicable law, the authors and copyright holders are not liable for claims, damages,
            data loss, disclosure, or other liability arising from the software, this integration,
            or your use of either. See the project&apos;s{" "}
            <a href="https://github.com/delexw/claude-code-trace/blob/main/LICENSE">MIT License</a>.
          </p>
          <p>
            This notice is informational and is not legal advice. A notice or disclaimer cannot
            eliminate every legal obligation or prevent a claim; obtain independent legal advice for
            your jurisdiction and use case.
          </p>
        </section>
        <label className="efficiency-privacy__confirmation">
          <input
            type="checkbox"
            checked={privacyConfirmed}
            onChange={(event) => setPrivacyConfirmed(event.target.checked)}
            disabled={busy}
          />
          I have reviewed the exact data and destination, am authorised to send it, and understand
          and accept the third-party processing, risks, and disclaimer above.
        </label>
        <div className="settings-modal__actions">
          <button type="button" className="settings-modal__btn" onClick={onCancel} disabled={busy}>
            Cancel
          </button>
          <button
            type="button"
            className="settings-modal__btn settings-modal__btn--primary"
            onClick={onConfirm}
            disabled={busy || !privacyConfirmed}
          >
            {busy ? "Starting…" : "Continue & Analyse"}
          </button>
        </div>
      </div>
    </PopoutModal>
  );
}

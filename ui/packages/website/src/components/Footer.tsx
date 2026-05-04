import { Link } from "react-router-dom";
import { DISCORD_URL, DOCS_URL, GITHUB_URL } from "../config";

const BRAND_NAME = "usezombie";

export default function Footer() {
  return (
    <footer className="site-footer">
      <div className="footer-grid">
        <div className="footer-brand">
          <span className="brand">{BRAND_NAME}</span>
          <p>Durable, markdown-defined agent runtime. BYOK. Open source.</p>
        </div>

        <div className="footer-col">
          <h4>Product</h4>
          <ul>
            <li><Link to="/">Features</Link></li>
            <li><Link to="/pricing">Pricing</Link></li>
            <li><a href={DOCS_URL} target="_blank" rel="noopener noreferrer">Docs</a></li>
            <li><Link to="/agents">Agents</Link></li>
          </ul>
        </div>

        <div className="footer-col">
          <h4>Community</h4>
          <ul>
            <li><a href={GITHUB_URL} target="_blank" rel="noopener noreferrer">GitHub</a></li>
            <li><a href={DISCORD_URL} target="_blank" rel="noopener noreferrer">Discord</a></li>
          </ul>
        </div>

        <div className="footer-col">
          <h4>Legal</h4>
          <ul>
            <li><Link to="/privacy">Privacy</Link></li>
            <li><Link to="/terms">Terms</Link></li>
          </ul>
        </div>
      </div>

      <div className="footer-bottom">
        <span>&copy; {new Date().getFullYear()} {BRAND_NAME}. All rights reserved.</span>
      </div>
    </footer>
  );
}

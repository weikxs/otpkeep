import "./base.css";
import { render } from "preact";
import { Shell } from "./Shell";

const root = document.getElementById("app");
if (root) {
  render(<Shell surface="app" />, root);
}

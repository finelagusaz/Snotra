import { render } from "solid-js/web";
import ResultsApp from "./ResultsApp";

const root = document.getElementById("root");
if (root) {
  render(() => <ResultsApp />, root);
}

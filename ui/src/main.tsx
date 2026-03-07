import { render } from "solid-js/web";
import MainApp from "./MainApp";

const root = document.getElementById("root");
if (root) {
  render(() => <MainApp />, root);
}

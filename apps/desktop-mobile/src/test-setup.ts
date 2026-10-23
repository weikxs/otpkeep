import { cleanup } from "@testing-library/preact";
import { afterEach } from "vitest";

// Unmount rendered components so each test starts from an empty document.
afterEach(cleanup);

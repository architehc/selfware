import { describe, it, expect } from "vitest";
import {
  delay,
  retryWithBackoff,
  promisePool,
  timeout,
  debounce,
} from "../src/asyncUtils.js";

describe("delay", () => {
  it("waits for specified time", async () => {
    const start = Date.now();
    await delay(50);
    const elapsed = Date.now() - start;
    expect(elapsed).toBeGreaterThanOrEqual(45);
  });
});

describe("retryWithBackoff", () => {
  it("returns result on success", async () => {
    const fn = () => Promise.resolve("success");
    const result = await retryWithBackoff(fn, 3);
    expect(result).toBe("success");
  });

  it("retries on failure and eventually succeeds", async () => {
    let attempts = 0;
    const fn = () => {
      attempts++;
      if (attempts < 3) return Promise.reject(new Error("fail"));
      return Promise.resolve("success");
    };
    const result = await retryWithBackoff(fn, 3, 10);
    expect(result).toBe("success");
    expect(attempts).toBe(3);
  });

  it("throws after max retries", async () => {
    const fn = () => Promise.reject(new Error("always fails"));
    await expect(retryWithBackoff(fn, 2, 10)).rejects.toThrow("always fails");
  });
});

describe("promisePool", () => {
  it("runs tasks with concurrency limit", async () => {
    let running = 0;
    let maxRunning = 0;

    const createTask = (id) => async () => {
      running++;
      maxRunning = Math.max(maxRunning, running);
      await delay(50);
      running--;
      return id;
    };

    const tasks = [1, 2, 3, 4, 5].map((id) => createTask(id));
    const results = await promisePool(tasks, 2);

    expect(results).toEqual([1, 2, 3, 4, 5]);
    expect(maxRunning).toBeLessThanOrEqual(2);
  });
});

describe("timeout", () => {
  it("returns result if promise resolves in time", async () => {
    const promise = delay(10).then(() => "done");
    const result = await timeout(promise, 100);
    expect(result).toBe("done");
  });

  it("throws if promise takes too long", async () => {
    const promise = delay(100).then(() => "done");
    await expect(timeout(promise, 10, "Too slow")).rejects.toThrow("Too slow");
  });
});

describe("debounce", () => {
  it("debounces function calls", async () => {
    let calls = 0;
    const fn = () => calls++;
    const debouncedFn = debounce(fn, 50);

    debouncedFn();
    debouncedFn();
    debouncedFn();
    expect(calls).toBe(0);

    await delay(75);
    expect(calls).toBe(1);

    debouncedFn();
    await delay(75);
    expect(calls).toBe(2);
  });
});

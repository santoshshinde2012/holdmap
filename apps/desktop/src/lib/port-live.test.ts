import { describe, expect, it } from "vitest";
import { MOCK_SNAPSHOT } from "./mock";
import { portHolderKeys } from "./port-live";

describe("port holder cache identity", () => {
  it("ignores live usage updates and listener order while tracking PID reuse", () => {
    const entries = structuredClone(MOCK_SNAPSHOT.entries);
    const original = portHolderKeys(entries);
    entries.reverse();
    for (const entry of entries) if (entry.process) { entry.process.memory_bytes++; entry.process.cpu_percent = 10; }
    expect(portHolderKeys(entries)).toEqual(original);
    const entry = entries.find((value) => value.port === 3000)!;
    entry.process!.start_token++;
    expect(portHolderKeys(entries).get(3000)).not.toBe(original.get(3000));
  });

  it("includes every holder sharing a port, across protocols and containers", () => {
    const entry = structuredClone(MOCK_SNAPSHOT.entries.find((value) => value.port === 3000)!);
    const second = { ...structuredClone(entry), id: "udp:3000", protocol: "udp" as const, pid: 5555, pids: [5555] };
    const before = portHolderKeys([entry, second]).get(3000);
    expect(portHolderKeys([second, entry]).get(3000)).toBe(before);
    second.pids.push(5556);
    expect(portHolderKeys([entry, second]).get(3000)).not.toBe(before);
    expect(portHolderKeys([entry]).get(3000)).not.toBe(before);
  });
});

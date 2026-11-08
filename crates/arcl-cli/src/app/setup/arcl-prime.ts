// Written by `arcl setup pi`. Runs `arcl prime` when a session starts and
// adds its output to the next prompt.
import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";

export default function arclPrime(pi: ExtensionAPI) {
	let primer: string | undefined;

	pi.on("session_start", async (_event, ctx) => {
		primer = undefined;
		try {
			const result = await pi.exec("arcl", ["prime"], { cwd: ctx.cwd, timeout: 10_000 });
			if (result.code === 0 && result.stdout.trim()) primer = result.stdout.trim();
		} catch {
			// arcl is not installed; start without the primer.
		}
	});

	pi.on("before_agent_start", async () => {
		if (!primer) return;
		const content = primer;
		primer = undefined;
		return { message: { customType: "arcl-prime", content, display: false } };
	});
}

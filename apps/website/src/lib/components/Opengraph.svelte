<!--
	The social card that `/og.png` renders through satori. Satori reads inline
	styles only, so the colors are the dark packages/ui tokens written out. It
	renders only the font weights `+server.ts` loads: Google Sans 400 and 700, and
	IBM Plex Sans 400.
-->
<script lang="ts">
	type Props = {
		width: number;
		height: number;
		faviconDataUrl: string;
	};

	let { width, height, faviconDataUrl }: Props = $props();
</script>

<div
	style="display:flex; position:relative; width:{width}px; height:{height}px; overflow:hidden; background:#09090b; color:#e6e7ea; font-family:'IBM Plex Sans';">
	<div
		style="display:flex; position:absolute; top:0; left:0; width:100%; height:100%; background-image:linear-gradient(125deg, #09090b 0%, #121316 58%, #152544 100%);">
	</div>
	<div
		style="display:flex; position:absolute; top:-190px; right:-105px; width:550px; height:550px; border:1px solid #6f737c; border-radius:275px; opacity:0.72;">
	</div>
	<div
		style="display:flex; position:absolute; top:-120px; right:-35px; width:410px; height:410px; border:1px solid #6f737c; border-radius:205px; opacity:0.66;">
	</div>
	<div
		style="display:flex; position:absolute; right:106px; top:82px; width:12px; height:12px; border-radius:6px; background:#4c8dff;">
	</div>
	<div
		style="display:flex; position:absolute; right:159px; top:121px; width:7px; height:7px; border-radius:4px; background:#79a9ff; opacity:0.75;">
	</div>

	<div style="display:flex; position:relative; width:100%; flex-direction:column; padding:48px 58px 50px;">
		<div
			style="display:flex; align-items:center; gap:13px; color:#79a9ff; font-family:'Google Sans'; font-size:23px; font-weight:400; letter-spacing:-0.4px;">
			<img src={faviconDataUrl} alt="" style="display:flex; width:31px; height:31px;" />
			<span>Arc Lightning</span>
		</div>

		<div style="display:flex; flex:1; margin-top:46px; flex-direction:column;">
			<div
				style="display:flex; color:#4c8dff; font-family:'Google Sans'; font-size:18px; font-weight:400; letter-spacing:2.2px;">
				LOCAL-FIRST PROJECT PLANNING
			</div>
			<div
				style="display:flex; max-width:600px; margin-top:10px; color:#f6f7f9; font-family:'Google Sans'; font-size:72px; font-weight:700; letter-spacing:-2.8px; line-height:0.98;">
				Turn intent into ready work.
			</div>
			<div style="display:flex; max-width:700px; margin-top:20px; color:#a0a4ac; font-size:25px; line-height:1.32;">
				Connected planning for developers and software agents.
			</div>
		</div>

		<div
			style="display:flex; justify-content:space-between; align-items:center; gap:17px; color:#a0a4ac; font-family:'IBM Plex Sans'; font-size:19px; line-height:1;">
			<div style="display:flex; align-items:center; gap:10px;">
				<div style="display:flex; width:8px; height:8px; border-radius:4px; background:#2563eb;"></div>
				<span>github.com/stormlightlabs/arclightning</span>
			</div>
			<div style="display:flex; align-items:center; gap:10px;">
				<div style="display:flex; width:8px; height:8px; border-radius:4px; background:#4c8dff;"></div>
				<span>arcl.stormlightlabs.org</span>
			</div>
		</div>
	</div>
</div>

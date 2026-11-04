<script lang="ts">
	import { resolve } from '$app/paths';
	import { SITE_NAME, SITE_URL } from '#lib/config.js';
	import Doc from '#lib/Doc.svelte';

	const title = `Privacy policy | ${SITE_NAME}`;
	const description =
		'Lamplit collects no data: no account, no analytics, no ads, no tracking. Your highlights, notes and bookmarks stay on your device.';
	const url = `${SITE_URL}/privacy`;
</script>

<svelte:head>
	<title>{title}</title>
	<meta name="description" content={description} />
	<link rel="canonical" href={url} />
</svelte:head>

<Doc heading="Privacy policy" meta="Updated 4 November 2026. Covers Lamplit for iPhone, iPad and Mac, and this website.">
	<p class="lede">
		Lamplit collects no data. There's no account, no analytics, no ads and no tracking. Your
		highlights, notes, bookmarks, searches and reading history are never sent to the developer.
	</p>

	<h2>What stays on your device</h2>
	<p>
		The app saves your highlights, notes, bookmarks, reading position, history and settings in a
		file inside it on your iPhone, iPad or Mac. They leave it only when you export or share them.
		On iPhone and iPad, your device's iCloud or computer backups include them, as they do for any
		app. Translations you import on the Mac stay on your Mac.
	</p>

	<h2>When the app uses the internet</h2>
	<p class="lede">
		Lamplit works entirely offline. The only time it connects to the internet is a one-off
		download of the reading voice, when you ask for it. No accounts, no analytics, no tracking, no
		update checks.
	</p>
	<p>
		The voice comes from this app's own release page on GitHub, which sees your IP address, as any
		download does. Every translation is inside the app: the seven bundled ones and any you convert
		from your own USFM, OSIS, Zefania or SWORD files. None of them is fetched, so no publisher or
		server ever learns what you are reading.
	</p>

	<h2>Sharing and crash reports</h2>
	<p>
		When you copy or share a verse, or export your notes, it goes only where you send it. Apple
		passes crash reports to developers only if you've turned on sharing with app developers in your
		device's settings. They describe the crash, not your notes or highlights.
	</p>

	<h2>Children</h2>
	<p>The app collects nothing from anyone, children included.</p>

	<h2>This website</h2>
	<p>
		This site sets no cookies and serves its own fonts. It counts page views with Vercel Web
		Analytics. For each view, Vercel records the page, the site that linked to it, your country,
		region and city, and your browser, operating system and device type. In the web reader, the
		page is the chapter it opens at. Vercel tells visits apart with a hash of the request, which it
		discards after 24 hours, and doesn't link the views to you or your IP address. The site also
		loads the download count from GitHub, which sees your IP address when it serves it.
	</p>
	<p>
		In the web reader, the NLT comes live from Tyndale. When you choose it, the reader asks this
		site for the chapter you're reading, and the site asks Tyndale's NLT API for it with the
		site's own key. Tyndale sees which chapter the site asked for, but not your browser or your IP
		address. Vercel's servers keep each chapter for up to a day, so the next reader of it
		doesn't ask Tyndale again. To keep the service from being overused, the site counts how many
		chapters each IP address asks for. The counts are kept in memory and deleted about a day after
		that address's first request. Your browser keeps only the chapter on screen, and only until
		you move on.
	</p>

	<h2>Changes and questions</h2>
	<p>
		Changes to this policy are published on this page with a new date. Questions are welcome on the
		<a href={resolve('/(site)/support')}>support page</a>.
	</p>

</Doc>

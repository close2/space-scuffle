# Q0081: A credential for the Gemini Omni trial, or stay on Veo?

- **Asked:** 2026-11-10
- **Blocking:** no
- **Default until answered:** stay on Veo 3.1 Fast for all media; the Omni trial is parked with $0.00 spent.

## Question

You asked (chat, 2026-11-09) whether we can and should switch away from Veo to Google's new video model. The new
model is **Gemini Omni 1.2 Flash** (`gemini-omni-1.2-flash`, GA 2026-11-05). We planned a capped trial of $4
(Veo plan, Amendment 2; evidence [`2026-11-10-omni-trial.md`](../../evidence/veo/2026-11-10-omni-trial.md)), and
the client for it is written and unit-tested.

**The trial stopped at its free first check.** Omni is served only through Vertex AI's Interactions API, and that
API refuses our Vertex express-mode API key, the only credential the pipeline has. Every check was free; nothing
was generated or billed:

- create an interaction (`POST .../projects/<project>/locations/global/interactions`, empty body): HTTP 401;
- read one (`GET .../interactions/<id>`): HTTP 401 `UNAUTHENTICATED`, "API keys are not supported by this API.
  Expected OAuth2 access token or other authentication credentials that assert a principal", reason
  `CREDENTIALS_MISSING`.

The same key keeps working for Veo, so this is not a key or billing problem. Interactions needs an OAuth2 principal.

Options:

1. **A service account for the trial.** A Google Cloud service-account JSON key (or `gcloud` user credentials) for
   the same project, with the role "Vertex AI User", stored outside the repository like the current key. The
   client would then get a short-lived access token and send it as a bearer token instead of the API key. This is
   a small code change, plus the $4 trial on our credit.
2. **A Gemini API key (AI Studio, `AIza…`).** Omni is also on the Gemini API (`generativelanguage.googleapis.com
   /v1beta/interactions`, about $0.07 per second at 720p). That is billed to a separate AI Studio account, not our
   Vertex credit, and Google does not document a seed there.
3. **Stay on Veo (recommended for now).** Nothing forces a switch: Veo 3.1 Fast is still generally available on
   Vertex AI. Omni's quality on our particular problems (camera push-in, light rays, walks that turn toward the
   camera) is unproven. The research found no test of those, and only Omni's price is clearly better: about 30 %
   less on the Gemini API, and the Vertex price was unreadable.

Recommendation: option 3, unless you want the `colossus_mech` Walk gap tried with another model. In that case,
option 1 keeps the spending on the same credit.

## Answer

**Option 3 (Stay on Veo 3.1 Fast for now; keep the Gemini Omni trial parked at $0.00 spent).**

**Technical insight:** Veo 3.1 Fast works directly with the existing Vertex Express API key, our unit sheets are already calibrated to Veo's visual style, and Gemini Omni 1.2 Flash's behavior on isometric RTS turntables and walk cycles is unproven. Keep the Omni client parked unless we later decide to provision a Service Account key for a specific unit gap.

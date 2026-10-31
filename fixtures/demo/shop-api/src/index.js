import express from "express";
const app = express();
app.get("/health", (_request, response) => response.json({ ok: true }));
app.listen(8080, () => console.log("shop-api demo listening on :8080"));

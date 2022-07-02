#!/usr/bin/env node
import Application from "./application";

const appInstance = Application.getInstance();

appInstance.main(process.argv);

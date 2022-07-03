import clear from 'clear';
import figlet from 'figlet';
import { program } from 'commander';
import optimizeFiles from './optimize-files';
import filterFiles from './filter-files';

class Application {
  private static instance: Application;

  private constructor() {}

  public static getInstance(): Application {
    if (!Application.instance) {
        Application.instance = new Application();
    }

    return Application.instance;
  }

  public async main(argv: Array<string>) {
    clear();
    console.log(figlet.textSync('esvg', { horizontalLayout: 'full' }));

    program
      .name('esvg')
      .version('1.1.0')
      .description('SVG Optimization Utility')
      .argument('<glob>', 'the input svg file(s) to optimize')
      .option('-p, --pretty', 'the output SVG will not me minified')
      .parse(argv);

    if (argv.length < 3) program.outputHelp();

    const options = program.opts();

    const files = filterFiles(program.args);

    await optimizeFiles(files, options.pretty);
  }
}

export default Application;

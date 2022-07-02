import { optimize, OptimizeOptions, Plugin } from 'svgo';
import generateHash from './generate-hash';
import SVG from './svg';

/**
 * Default options for the svgo plugin
 * Reference: https://www.npmjs.com/package/svgo
 */
const defaultOptions: OptimizeOptions = {
  plugins: [
    {
      name: 'preset-default',
      params: {
        overrides: {
          removeViewBox: false,
        },
      },
    },
    { name: 'sortAttrs' },
    { name: 'removeDimensions' }
  ]
};

class SVGOAdapter {
  static optimize(inputSVG: SVG): SVG {
    const options: OptimizeOptions = defaultOptions;

    const prefixIdsPlugin: Plugin = {
      name: 'prefixIds',
      params: {
        prefix: generateHash()
      }
    };

    options.plugins.push(prefixIdsPlugin);

    const result: any = optimize(inputSVG.toString(), options);

    if (result.error) throw new Error(result.error.toString());

    if (!result.data) throw new Error('No result data from the SVGO optimization');

    return new SVG(result.data);
  }
}

export default SVGOAdapter;

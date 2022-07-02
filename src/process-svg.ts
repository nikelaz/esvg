import SVG from './svg';
import SVGOAdapter from './svgo-adapter';
import SVGFlattenAdapter from './svg-flatten-adapter';
import removeTransforms from './remove-transforms';

function processSvg(inputSvg: SVG, pretty: Boolean = false): SVG {
  // Optimize the svg with svgo
  const optimizedSvg = SVGOAdapter.optimize(inputSvg, true);

  // Convert all shapes in the svg to paths
  const flattenedSvg = SVGFlattenAdapter.flatten(optimizedSvg);

  // Remove transforms from the svg
  const svgWithoutTransforms = removeTransforms(flattenedSvg);

  // Optimize the svg finally with svgo (2nd time)
  return SVGOAdapter.optimize(svgWithoutTransforms, false, pretty);
}

export default processSvg;

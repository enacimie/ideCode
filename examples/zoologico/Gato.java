public class Gato extends Animal implements Mascota {
    private int vidas;

    public Gato(String nombre, int edad) {
        super(nombre, edad);
        this.vidas = 7;
    }

    @Override
    public String hablar() {
        return "Miau";
    }

    @Override
    public void jugar() {
        System.out.println(nombre + " juega con un ovillo.");
    }

    public int getVidas() {
        return vidas;
    }
}